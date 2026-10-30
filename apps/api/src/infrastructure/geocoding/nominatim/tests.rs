use super::*;
use axum::{Router, body::Body, extract::State, http::Request, response::Response};
use sqlx::PgPool;
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

const POINT: &str = r#"[{"lon":"77.61","lat":"12.95","name":"Bengaluru","display_name":"Bengaluru, India","type":"city","address":{"country_code":"in"}}]"#;

#[derive(Default)]
struct Provider {
    requests: Mutex<Vec<(String, String)>>,
    replies: Mutex<VecDeque<(u16, String, Option<String>)>>,
    delay_ms: AtomicU64,
    chunked: AtomicBool,
}

async fn respond(State(provider): State<Arc<Provider>>, request: Request<Body>) -> Response {
    provider.requests.lock().unwrap().push((
        request.uri().to_string(),
        request.headers()[header::USER_AGENT]
            .to_str()
            .unwrap()
            .to_owned(),
    ));
    tokio::time::sleep(Duration::from_millis(
        provider.delay_ms.load(Ordering::Relaxed),
    ))
    .await;
    let (status, body, retry_after) =
        provider
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or((200, POINT.into(), None));
    let mut response = Response::builder()
        .status(status)
        .header(header::LOCATION, "/redirect");
    if let Some(value) = retry_after {
        response = response.header(header::RETRY_AFTER, value);
    }
    let body = if provider.chunked.load(Ordering::Relaxed) {
        let chunks: Vec<_> = body
            .as_bytes()
            .chunks(64)
            .map(|part| Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(part)))
            .collect();
        Body::from_stream(futures_util::stream::iter(chunks))
    } else {
        Body::from(body)
    };
    response.body(body).unwrap()
}

struct Fixture {
    geocoder: Geocoder,
    provider: Arc<Provider>,
    server: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new(db: PgPool) -> Self {
        let provider = Arc::new(Provider::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/search", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let router = Router::new().fallback(respond).with_state(provider.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let limits = crate::limits::Limits::bundled().unwrap().geocoding;
        let mut geocoder = Geocoder::new(db, limits, "ttl-geocoding-test").unwrap();
        geocoder.endpoint = endpoint;
        geocoder.client = Client::builder()
            .no_proxy()
            .user_agent("ttl-geocoding-test")
            .timeout(Duration::from_millis(
                geocoder.limits.request_timeout_ms.into(),
            ))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .unwrap();
        Self {
            geocoder,
            provider,
            server,
        }
    }

    fn count(&self) -> usize {
        self.provider.requests.lock().unwrap().len()
    }

    fn reply(&self, status: u16, body: &str, retry_after: Option<&str>) {
        self.provider.replies.lock().unwrap().push_back((
            status,
            body.into(),
            retry_after.map(str::to_owned),
        ));
    }

    async fn ready(&self) {
        sqlx::query("UPDATE geocoding_gate SET lease_id = NULL, next_allowed_at = '-infinity'")
            .execute(&self.geocoder.db)
            .await
            .unwrap();
    }

    async fn entries(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM geocoding_cache")
            .fetch_one(&self.geocoder.db)
            .await
            .unwrap()
    }

    async fn close(self) {
        self.server.abort();
        self.geocoder.db.close().await;
    }
}

#[sqlx::test]
async fn postal_cache_survives_clients_and_normalizes_country_and_spacing(pool: PgPool) {
    let f = Fixture::new(pool).await;
    let point = f.geocoder.postal_code(" sw1a 1aa ", " GB ").await.unwrap();
    assert_eq!(point.coordinates, vec![77.61, 12.95]);
    let mut second = Geocoder::new(
        f.geocoder.db.clone(),
        f.geocoder.limits.clone(),
        "other-replica",
    )
    .unwrap();
    second.endpoint = f.geocoder.endpoint.clone();
    second.postal_code("SW1A1AA", "gb").await.unwrap();
    assert_eq!(f.count(), 1);
    assert_eq!(f.entries().await, 1);
    let requests = f.provider.requests.lock().unwrap().clone();
    assert!(requests[0].0.contains("postalcode=SW1A1AA"));
    assert!(requests[0].0.contains("countrycodes=gb"));
    assert_eq!(requests[0].1, "ttl-geocoding-test");
    assert!(matches!(
        f.geocoder.postal_code("SW1A1AA", "IN").await,
        Err(GeocodeError::RateLimited)
    ));
    f.ready().await;
    f.geocoder.postal_code("SW1A1AA", "IN").await.unwrap();
    assert_eq!(f.count(), 2);
    assert_eq!(f.entries().await, 2);
    f.close().await;
}

#[sqlx::test]
async fn empty_results_are_cached_briefly_and_expiry_refreshes(pool: PgPool) {
    let f = Fixture::new(pool).await;
    f.reply(200, "[]", None);
    for _ in 0..2 {
        assert!(matches!(
            f.geocoder.postal_code("560025", "IN").await,
            Err(GeocodeError::NoResults)
        ));
    }
    assert_eq!(f.count(), 1);
    let remaining: f64 = sqlx::query_scalar(
        "SELECT extract(epoch FROM expires_at - clock_timestamp())::float8 FROM geocoding_cache",
    )
    .fetch_one(&f.geocoder.db)
    .await
    .unwrap();
    assert!(remaining > 3500.0 && remaining <= 3600.0);
    sqlx::query("UPDATE geocoding_cache SET expires_at = clock_timestamp() - interval '1 second'")
        .execute(&f.geocoder.db)
        .await
        .unwrap();
    f.ready().await;
    f.geocoder.postal_code("560025", "IN").await.unwrap();
    assert_eq!(f.count(), 2);
    let remaining: f64 = sqlx::query_scalar(
        "SELECT extract(epoch FROM expires_at - clock_timestamp())::float8 FROM geocoding_cache",
    )
    .fetch_one(&f.geocoder.db)
    .await
    .unwrap();
    assert!(remaining > 2591900.0);
    f.close().await;
}

#[sqlx::test]
async fn replicas_and_city_discovery_share_one_gate(pool: PgPool) {
    let f = Fixture::new(pool).await;
    f.provider.delay_ms.store(100, Ordering::Relaxed);
    let replicas: Vec<_> = (0..10)
        .map(|_| {
            let g = f.geocoder.clone();
            tokio::spawn(async move { g.postal_code("560025", "IN").await })
        })
        .collect();
    let mut successes = 0;
    for task in replicas {
        match task.await.unwrap() {
            Ok(_) => successes += 1,
            Err(GeocodeError::RateLimited) => {}
            other => panic!("unexpected lookup: {other:?}"),
        }
    }
    assert!(successes >= 1);
    assert_eq!(f.count(), 1);
    assert!(matches!(
        f.geocoder.cities("London", Some("GB"), 10).await,
        Err(GeocodeError::RateLimited)
    ));
    tokio::time::sleep(Duration::from_millis(1150)).await;
    f.geocoder
        .cities("  LONDON  ", Some("gb"), 10)
        .await
        .unwrap();
    f.geocoder.cities("London", Some("GB"), 10).await.unwrap();
    assert_eq!(f.count(), 2);
    assert_eq!(f.entries().await, 2);
    f.close().await;
}

#[sqlx::test]
async fn failures_are_not_results_and_retry_after_blocks_other_queries(pool: PgPool) {
    let f = Fixture::new(pool).await;
    for (status, body, expected) in [
        (429, "[]", "provider_rate_limited"),
        (503, "[]", "provider_unavailable"),
        (302, "[]", "provider_unavailable"),
        (200, "not json", "invalid_response"),
        (200, r#"[{"lon":"NaN","lat":"12"}]"#, "invalid_response"),
        (200, r#"[{"lon":"77","lat":"91"}]"#, "invalid_response"),
    ] {
        f.ready().await;
        f.reply(status, body, Some("120"));
        let before = f.count();
        assert_eq!(
            f.geocoder
                .postal_code("560025", "IN")
                .await
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(f.count(), before + 1);
        assert_eq!(f.entries().await, 0);
        assert!(matches!(
            f.geocoder.cities("London", None, 5).await,
            Err(GeocodeError::RateLimited)
        ));
        assert_eq!(f.count(), before + 1);
        if status == 429 || status == 503 {
            let cooldown: f64 = sqlx::query_scalar("SELECT extract(epoch FROM next_allowed_at - clock_timestamp())::float8 FROM geocoding_gate")
                .fetch_one(&f.geocoder.db).await.unwrap();
            assert!(cooldown > 119.0 && cooldown <= 120.0);
        }
    }
    f.close().await;
}

#[sqlx::test]
async fn cache_capacity_is_bounded_and_expired_rows_are_removed(pool: PgPool) {
    let mut f = Fixture::new(pool).await;
    f.geocoder.limits.max_entries = 2;
    for postal in ["100001", "100002", "100003"] {
        f.ready().await;
        f.geocoder.postal_code(postal, "IN").await.unwrap();
    }
    assert_eq!(f.entries().await, 2);
    f.geocoder.postal_code("100003", "IN").await.unwrap();
    assert_eq!(f.count(), 3);
    f.ready().await;
    f.geocoder.postal_code("100001", "IN").await.unwrap();
    assert_eq!(f.count(), 4);
    assert_eq!(f.entries().await, 2);
    sqlx::query("UPDATE geocoding_cache SET expires_at = '-infinity'")
        .execute(&f.geocoder.db)
        .await
        .unwrap();
    f.ready().await;
    f.geocoder.postal_code("100004", "IN").await.unwrap();
    assert_eq!(f.entries().await, 1);
    f.close().await;
}

#[sqlx::test]
async fn cancellation_keeps_lease_until_recovery(pool: PgPool) {
    let f = Fixture::new(pool).await;
    f.provider.delay_ms.store(1000, Ordering::Relaxed);
    let g = f.geocoder.clone();
    let task = tokio::spawn(async move { g.postal_code("560025", "IN").await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while f.count() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    task.abort();
    let _ = task.await;
    assert!(matches!(
        f.geocoder.postal_code("560026", "IN").await,
        Err(GeocodeError::RateLimited)
    ));
    assert_eq!(f.count(), 1);
    f.ready().await;
    f.provider.delay_ms.store(0, Ordering::Relaxed);
    f.geocoder.postal_code("560026", "IN").await.unwrap();
    assert_eq!(f.entries().await, 1);
    f.close().await;
}

#[sqlx::test]
async fn timeouts_and_database_failures_do_not_bypass_gate(pool: PgPool) {
    let mut f = Fixture::new(pool).await;
    f.geocoder.client = Client::builder()
        .no_proxy()
        .user_agent("test")
        .timeout(Duration::from_millis(200))
        .build()
        .unwrap();
    f.provider.delay_ms.store(500, Ordering::Relaxed);
    assert!(matches!(
        f.geocoder.postal_code("560025", "IN").await,
        Err(GeocodeError::Unavailable)
    ));
    assert!(matches!(
        f.geocoder.postal_code("560026", "IN").await,
        Err(GeocodeError::RateLimited)
    ));
    assert_eq!(f.count(), 1);
    assert_eq!(f.entries().await, 0);
    sqlx::query("DROP TABLE geocoding_gate")
        .execute(&f.geocoder.db)
        .await
        .unwrap();
    assert!(matches!(
        f.geocoder.postal_code("560027", "IN").await,
        Err(GeocodeError::Database(_))
    ));
    assert_eq!(f.count(), 1);
    f.close().await;
}

#[sqlx::test]
async fn oversized_and_streamed_responses_are_rejected(pool: PgPool) {
    let mut f = Fixture::new(pool).await;
    f.geocoder.limits.max_response_bytes = 128;
    f.reply(200, &" ".repeat(129), None);
    assert!(matches!(
        f.geocoder.postal_code("560025", "IN").await,
        Err(GeocodeError::InvalidResponse)
    ));
    f.ready().await;
    f.provider.chunked.store(true, Ordering::Relaxed);
    f.reply(200, &" ".repeat(129), None);
    assert!(matches!(
        f.geocoder.postal_code("560026", "IN").await,
        Err(GeocodeError::InvalidResponse)
    ));
    assert_eq!(f.count(), 2);
    assert_eq!(f.entries().await, 0);
    f.close().await;
}

#[sqlx::test]
async fn invalid_queries_never_contact_provider(pool: PgPool) {
    let f = Fixture::new(pool).await;
    for (postal, country) in [
        ("", "IN"),
        ("x&limit=20", "IN"),
        ("123", "IND"),
        ("123", "12"),
    ] {
        assert!(matches!(
            f.geocoder.postal_code(postal, country).await,
            Err(GeocodeError::InvalidQuery)
        ));
    }
    assert!(matches!(
        f.geocoder.cities("  ", None, 10).await,
        Err(GeocodeError::InvalidQuery)
    ));
    assert_eq!(f.count(), 0);
    f.close().await;
}

#[test]
fn retry_after_accepts_seconds_and_http_dates() {
    let mut headers = header::HeaderMap::new();
    headers.insert(header::RETRY_AFTER, "120".parse().unwrap());
    assert_eq!(retry_after_ms(&headers), Some(120000));
    let date = (chrono::Utc::now() + chrono::Duration::seconds(120)).to_rfc2822();
    headers.insert(header::RETRY_AFTER, date.parse().unwrap());
    assert!((119000..=120000).contains(&retry_after_ms(&headers).unwrap()));
    headers.insert(header::RETRY_AFTER, "bad".parse().unwrap());
    assert_eq!(retry_after_ms(&headers), None);
}

#[sqlx::test]
async fn total_timeout_leaves_gate_closed_until_lease_expires(pool: PgPool) {
    let mut f = Fixture::new(pool).await;
    f.geocoder.limits.total_timeout_ms = 500;
    f.provider.delay_ms.store(2000, Ordering::Relaxed);
    assert!(matches!(
        f.geocoder.postal_code("560025", "IN").await,
        Err(GeocodeError::Unavailable)
    ));
    assert!(matches!(
        f.geocoder.postal_code("560026", "IN").await,
        Err(GeocodeError::RateLimited)
    ));
    assert_eq!(f.count(), 1);
    let remaining: f64 = sqlx::query_scalar("SELECT extract(epoch FROM next_allowed_at - clock_timestamp())::float8 FROM geocoding_gate")
        .fetch_one(&f.geocoder.db).await.unwrap();
    assert!(remaining > 0.0 && remaining < 1.6);
    f.close().await;
}

#[sqlx::test]
async fn stale_request_cannot_overwrite_or_release_newer_lease(pool: PgPool) {
    let f = Fixture::new(pool).await;
    let old = Uuid::now_v7();
    let current = Uuid::now_v7();
    sqlx::query("UPDATE geocoding_gate SET lease_id = $1, next_allowed_at = clock_timestamp() + interval '1 hour'")
        .bind(current).execute(&f.geocoder.db).await.unwrap();
    f.geocoder.release(old, 0).await.unwrap();
    assert!(matches!(
        f.geocoder.store(old, &"0".repeat(64), &[]).await,
        Err(GeocodeError::RateLimited)
    ));
    assert!(matches!(
        f.geocoder.postal_code("560025", "IN").await,
        Err(GeocodeError::RateLimited)
    ));
    assert_eq!(f.entries().await, 0);
    assert_eq!(f.count(), 0);
    f.close().await;
}

#[sqlx::test]
async fn connection_failure_starts_cooldown_without_caching(pool: PgPool) {
    let mut f = Fixture::new(pool).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    f.geocoder.endpoint = format!("http://{}/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    drop(listener);
    assert!(matches!(
        f.geocoder.postal_code("560025", "IN").await,
        Err(GeocodeError::Unavailable)
    ));
    assert!(matches!(
        f.geocoder.cities("London", None, 5).await,
        Err(GeocodeError::RateLimited)
    ));
    assert_eq!(f.entries().await, 0);
    f.close().await;
}
