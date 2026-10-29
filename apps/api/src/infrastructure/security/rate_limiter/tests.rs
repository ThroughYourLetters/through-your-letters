use super::*;
use crate::limits::Limits;
use uuid::Uuid;

async fn managed(limiter: &RateLimiter) -> ConnectionManager {
    let slot = limiter.connection.read().await.clone();
    limiter.connection(&slot).await.unwrap()
}

#[tokio::test]
async fn claims_are_atomic_expire_and_reuse_the_connection() {
    let client =
        Client::open(std::env::var("TEST_REDIS_URL").expect("disposable Redis required")).unwrap();
    let limiter = std::sync::Arc::new(RateLimiter::new(
        client,
        &Limits::bundled().unwrap().security,
    ));
    let key = Uuid::now_v7().to_string();
    let policy = RateLimitPolicy {
        requests: 3,
        window_seconds: 60,
    };
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let limiter = limiter.clone();
        let key = key.clone();
        let policy = policy.clone();
        tasks.spawn(async move { limiter.check(&key, &policy).await.unwrap() });
    }
    let mut admitted = 0;
    while let Some(result) = tasks.join_next().await {
        admitted += usize::from(result.unwrap());
    }
    assert_eq!(admitted, 3);
    let mut connection = managed(&limiter).await;
    let first: i64 = redis::cmd("CLIENT")
        .arg("ID")
        .query_async(&mut connection)
        .await
        .unwrap();
    let expiring = Uuid::now_v7().to_string();
    let short = RateLimitPolicy {
        requests: 1,
        window_seconds: 1,
    };
    assert!(limiter.check(&expiring, &short).await.unwrap());
    assert!(!limiter.check(&expiring, &short).await.unwrap());
    let ttl: i64 = redis::cmd("TTL")
        .arg(format!("abuse:{expiring}"))
        .query_async(&mut connection)
        .await
        .unwrap();
    assert!((0..=1).contains(&ttl));
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(limiter.check(&expiring, &short).await.unwrap());
    let mut reused = managed(&limiter).await;
    let second: i64 = redis::cmd("CLIENT")
        .arg("ID")
        .query_async(&mut reused)
        .await
        .unwrap();
    assert_eq!(first, second);
}

#[tokio::test]
async fn lost_connections_recover_and_unresponsive_backends_have_a_deadline() {
    let client =
        Client::open(std::env::var("TEST_REDIS_URL").expect("disposable Redis required")).unwrap();
    let limiter = RateLimiter::new(client.clone(), &Limits::bundled().unwrap().security);
    let policy = RateLimitPolicy {
        requests: 100,
        window_seconds: 60,
    };
    let key = Uuid::now_v7().to_string();
    assert!(limiter.check(&key, &policy).await.unwrap());
    let mut managed = managed(&limiter).await;
    let id: i64 = redis::cmd("CLIENT")
        .arg("ID")
        .query_async(&mut managed)
        .await
        .unwrap();
    let mut control = client.get_multiplexed_async_connection().await.unwrap();
    let _: i64 = redis::cmd("CLIENT")
        .arg("KILL")
        .arg("ID")
        .arg(id)
        .query_async(&mut control)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if limiter.check(&key, &policy).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("rate limiter should reconnect");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stalled = Client::open(format!("redis://{}", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
    });
    let mut limits = Limits::bundled().unwrap().security;
    limits.redis_connect_timeout_ms = 100;
    limits.redis_response_timeout_ms = 100;
    let stalled = RateLimiter::new(stalled, &limits);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), stalled.check(&key, &policy))
            .await
            .unwrap()
            .is_err()
    );
    server.abort();
}

#[tokio::test]
async fn backend_errors_and_invalid_policies_are_not_permissions() {
    let client =
        Client::open(std::env::var("TEST_REDIS_URL").expect("disposable Redis required")).unwrap();
    let limiter = RateLimiter::new(client, &Limits::bundled().unwrap().security);
    let mut connection = managed(&limiter).await;
    let key = Uuid::now_v7().to_string();
    let _: () = redis::cmd("SET")
        .arg(format!("abuse:{key}"))
        .arg("not a counter")
        .arg("EX")
        .arg(5)
        .query_async(&mut connection)
        .await
        .unwrap();
    assert!(
        limiter
            .check(
                &key,
                &RateLimitPolicy {
                    requests: 1,
                    window_seconds: 5
                }
            )
            .await
            .is_err()
    );
    assert!(
        limiter
            .check(
                &key,
                &RateLimitPolicy {
                    requests: 0,
                    window_seconds: 5
                }
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn lost_responses_fail_once_then_reconnect_without_replaying_the_claim() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    let redis_url = std::env::var("TEST_REDIS_URL").expect("disposable Redis required");
    let upstream = reqwest::Url::parse(&redis_url).unwrap();
    let destination = (
        upstream.host_str().unwrap().to_string(),
        upstream.port().unwrap_or(6379),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut proxy_url = upstream.clone();
    proxy_url.set_host(Some("127.0.0.1")).unwrap();
    proxy_url
        .set_port(Some(listener.local_addr().unwrap().port()))
        .unwrap();
    let drop_response = Arc::new(AtomicBool::new(false));
    let connections = Arc::new(AtomicUsize::new(0));
    let dropping = drop_response.clone();
    let accepted = connections.clone();
    let server = tokio::spawn(async move {
        let mut tasks = tokio::task::JoinSet::new();
        loop {
            let (client, _) = listener.accept().await.unwrap();
            let first = accepted.fetch_add(1, Ordering::SeqCst) == 0;
            let dropping = dropping.clone();
            let upstream = TcpStream::connect((destination.0.as_str(), destination.1))
                .await
                .unwrap();
            tasks.spawn(async move {
                let (mut client_read, mut client_write) = client.into_split();
                let (mut upstream_read, mut upstream_write) = upstream.into_split();
                tokio::select! {
                    _ = tokio::io::copy(&mut client_read, &mut upstream_write) => {}
                    _ = async {
                        let mut bytes = [0; 4096];
                        loop {
                            let count = upstream_read.read(&mut bytes).await?;
                            if count == 0 { return Ok::<(), std::io::Error>(()); }
                            if !first || !dropping.load(Ordering::SeqCst) {
                                client_write.write_all(&bytes[..count]).await?;
                            }
                        }
                    } => {}
                }
            });
        }
    });
    let mut limits = Limits::bundled().unwrap().security;
    limits.redis_connect_timeout_ms = 500;
    limits.redis_response_timeout_ms = 100;
    let limiter = RateLimiter::new(Client::open(proxy_url.as_str()).unwrap(), &limits);
    let policy = RateLimitPolicy {
        requests: 3,
        window_seconds: 60,
    };
    assert!(
        limiter
            .check(&Uuid::now_v7().to_string(), &policy)
            .await
            .unwrap()
    );
    drop_response.store(true, Ordering::SeqCst);
    let key = Uuid::now_v7().to_string();
    assert!(limiter.check(&key, &policy).await.unwrap_err().is_timeout());

    let mut control = Client::open(redis_url)
        .unwrap()
        .get_multiplexed_async_connection()
        .await
        .unwrap();
    let count: i64 = redis::cmd("GET")
        .arg(format!("abuse:{key}"))
        .query_async(&mut control)
        .await
        .unwrap();
    assert_eq!(count, 1, "a lost response must not replay the EVAL");
    assert!(
        limiter.check(&key, &policy).await.unwrap(),
        "the next request must use a fresh socket"
    );
    assert_eq!(connections.load(Ordering::SeqCst), 2);
    server.abort();
    let _ = server.await;
}
