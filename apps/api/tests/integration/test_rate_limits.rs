use super::helpers::{expect_status, send, spawn_app, unique_email};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::json;

async fn register(app: &Router, expected: StatusCode) {
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "email": unique_email("rate-limit-it"),
                "password": "StrongerPass123!",
                "display_name": "Rate Limit Tester"
            })
            .to_string(),
        ))
        .expect("failed to build register request");
    expect_status(send(app, request).await, expected).await;
}

#[tokio::test]
async fn registration_limits_apply_per_fixture_client() {
    let first = spawn_app().await;
    for _ in 0..3 {
        register(&first.app, StatusCode::OK).await;
    }
    register(&first.app, StatusCode::TOO_MANY_REQUESTS).await;

    let second = spawn_app().await;
    register(&second.app, StatusCode::OK).await;
}
