use super::helpers::{expect_status, read_json, send, spawn_app};
use axum::{body::Body, http::Request, http::StatusCode};
use serde_json::Value;
use uuid::Uuid;

#[sqlx::test]
async fn saved_city_search_survives_provider_failure_and_discovery_reports_it(pool: sqlx::PgPool) {
    let app = spawn_app(&pool).await;
    app.stop_geocoder();
    let query = format!("GeocodingFixture{}", Uuid::now_v7().simple());
    let response = send(
        &app.app,
        Request::builder()
            .uri(format!("/api/v1/cities?q={query}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let response = expect_status(response, StatusCode::OK).await;
    assert!(read_json::<Vec<Value>>(response).await.is_empty());

    let response = send(
        &app.app,
        Request::builder()
            .uri(format!("/api/v1/cities?q={query}&discover=true"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    expect_status(response, StatusCode::SERVICE_UNAVAILABLE).await;
}
