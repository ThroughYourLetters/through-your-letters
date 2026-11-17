use super::helpers::{expect_status, read_json, send, spawn_app_with_config};
use api::presentation::http::middleware::admin::AdminClaims;
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    routing::get,
};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Default)]
struct Provider {
    capitals: AtomicUsize,
    summaries: AtomicUsize,
    oversized: AtomicBool,
    delayed: AtomicBool,
}
struct Server(tokio::task::JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn provider() -> (String, Arc<Provider>, Server) {
    let state = Arc::new(Provider::default());
    let router = Router::new().route("/capitals", get(|State(state): State<Arc<Provider>>| async move {
        state.capitals.fetch_add(1, Ordering::SeqCst);
        if state.oversized.load(Ordering::SeqCst) { return Json(json!({"padding":"x".repeat(2048)})); }
        Json(Value::Array((0..5).map(|i| json!({"cca2":"GB","name":{"common":"Britain"},"capital":[format!("Budget capital {i}")],"capitalInfo":{"latlng":[51.0+i as f64/10.0,0.0]}})).collect()))
    })).route("/wiki/{title}", get(|State(state): State<Arc<Provider>>| async move {
        state.summaries.fetch_add(1, Ordering::SeqCst);
        if state.delayed.load(Ordering::SeqCst) { tokio::time::sleep(Duration::from_secs(2)).await; }
        Json(json!({"extract":"City summary"}))
    })).with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (origin, state, Server(server))
}
fn request(body: Value) -> Request<Body> {
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &AdminClaims {
            sub: "budget-admin".into(),
            exp: (chrono::Utc::now().timestamp() + 3600) as usize,
            aud: "admin".into(),
        },
        &jsonwebtoken::EncodingKey::from_secret(b"test-admin-jwt-secret"),
    )
    .unwrap();
    Request::builder()
        .method("POST")
        .uri("/api/v1/admin/cities/bootstrap-capitals")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
#[sqlx::test]
async fn capital_imports_obey_page_response_and_enrichment_budgets(pool: sqlx::PgPool) {
    let (origin, provider, _server) = provider().await;
    let app = spawn_app_with_config(&pool, |config| {
        config.limits.city_services.capitals_url = format!("{origin}/capitals");
        config.limits.city_services.wikipedia_url = format!("{origin}/wiki/");
        config.limits.city_services.max_response_bytes = 1024;
        config.limits.city_services.max_enrichments = 1;
        config.limits.pages.capitals.default_limit = 2;
        config.limits.pages.capitals.max_limit = 3;
    })
    .await;
    let result: Value =
        read_json(expect_status(send(&app.app, request(json!({}))).await, StatusCode::OK).await)
            .await;
    assert_eq!(result, json!({"processed":2,"upserted":2,"failed":0}));
    assert_eq!(provider.capitals.load(Ordering::SeqCst), 1);
    assert_eq!(provider.summaries.load(Ordering::SeqCst), 1);
    expect_status(
        send(&app.app, request(json!({"limit":4}))).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    assert_eq!(provider.capitals.load(Ordering::SeqCst), 1);
    provider.oversized.store(true, Ordering::SeqCst);
    expect_status(
        send(&app.app, request(json!({}))).await,
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM cities WHERE name LIKE 'Budget capital %'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 2);
}
#[sqlx::test]
async fn slow_city_summaries_cannot_extend_the_import_deadline(pool: sqlx::PgPool) {
    let (origin, provider, _server) = provider().await;
    provider.delayed.store(true, Ordering::SeqCst);
    let app = spawn_app_with_config(&pool, |config| {
        config.limits.city_services.capitals_url = format!("{origin}/capitals");
        config.limits.city_services.wikipedia_url = format!("{origin}/wiki/");
        config.limits.city_services.request_timeout_ms = 100;
        config.limits.city_services.total_timeout_ms = 300;
        config.limits.city_services.max_enrichments = 5;
        config.limits.geocoding.request_timeout_ms = 100;
        config.limits.geocoding.total_timeout_ms = 200;
        config.limits.pages.capitals.default_limit = 5;
    })
    .await;
    let response = tokio::time::timeout(Duration::from_secs(2), send(&app.app, request(json!({}))))
        .await
        .unwrap();
    let result: Value = read_json(expect_status(response, StatusCode::OK).await).await;
    assert_eq!(result["failed"], 1);
    assert!(result["upserted"].as_u64().unwrap() < 5);
    assert!(provider.summaries.load(Ordering::SeqCst) < 5);
}

#[sqlx::test]
async fn summary_result_budgets_reach_public_http_responses(pool: sqlx::PgPool) {
    let mut fixture = super::test_pagination::Fixture::new(&pool).await;
    fixture.app = spawn_app_with_config(&pool, |config| {
        config.limits.summaries.leaderboard = 1;
        config.limits.summaries.neighborhoods = 1;
        config.limits.summaries.city_neighborhoods = 1;
        config.limits.summaries.related_photos = 1;
    })
    .await;
    sqlx::query("UPDATE letterings SET contributor_tag='Contributor '||right(id::text,4),pin_code=right(id::text,4),ml_style='Painted'")
        .execute(&pool).await.unwrap();
    let city: uuid::Uuid = sqlx::query_scalar("SELECT city_id FROM letterings LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    for (path, field) in [
        ("/api/v1/community/leaderboard".to_string(), ""),
        (
            "/api/v1/analytics/neighborhoods".to_string(),
            "neighborhoods",
        ),
        (format!("/api/v1/cities/{city}/stats"), ""),
        (
            format!("/api/v1/letterings/{}/similar", fixture.ids[0]),
            "similar",
        ),
    ] {
        let body = fixture.get(&path, false).await;
        let rows = if field.is_empty() {
            &body
        } else {
            &body[field]
        };
        assert_eq!(rows.as_array().unwrap().len(), 1, "{path}");
    }
}
