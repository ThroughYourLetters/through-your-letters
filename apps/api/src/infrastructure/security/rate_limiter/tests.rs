use super::*;
use crate::limits::Limits;
use uuid::Uuid;

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
    let mut connection = limiter.connection().await.unwrap();
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
    let mut reused = limiter.connection().await.unwrap();
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
    let mut managed = limiter.connection().await.unwrap();
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
    let mut connection = limiter.connection().await.unwrap();
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
