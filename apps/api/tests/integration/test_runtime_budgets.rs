use super::helpers::{
    expect_status, multipart_upload_body, read_json, send, spawn_app_with_config, tiny_png_bytes,
    unique_email,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::Utc;
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::time::Duration;

#[sqlx::test]
async fn configured_json_budgets_reject_large_and_slow_bodies_without_limiting_uploads(
    pool: PgPool,
) {
    let app = spawn_app_with_config(&pool, |config| {
        config.limits.http.json_max_bytes = 4096;
        config.limits.http.bounded_json_max_bytes = 1024;
        config.limits.http.body_read_timeout_ms = 100;
    })
    .await;
    for path in [
        "/api/v1/auth/register",
        "/api/v1/auth/forgot-password",
        "/api/v1/collections",
    ] {
        let response = send(&app.app, Request::builder().method("POST").uri(path)
            .header("content-type", "application/json")
            .body(Body::from(json!({"email":"body@example.test","password":"GoodPassword123!","name":"budget","description":"x".repeat(5000)}).to_string())).unwrap()).await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE, "{path}");
        let request = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from_stream(futures_util::stream::pending::<
                Result<bytes::Bytes, std::io::Error>,
            >()))
            .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(2), send(&app.app, request))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT, "{path}");
        let body: Value = read_json(response).await;
        assert_eq!(body["error"], "Request body timed out");
    }
    let city: uuid::Uuid = sqlx::query_scalar("SELECT id FROM cities LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut image = tiny_png_bytes();
    image.extend(vec![0; 8192]);
    let (boundary, body) = multipart_upload_body(
        "BudgetTester",
        "560001",
        "Valid upload",
        &city.to_string(),
        &image,
    );
    assert!(body.len() > 4096);
    let response = send(
        &app.app,
        Request::builder()
            .method("POST")
            .uri("/api/v1/letterings/upload")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .unwrap(),
    )
    .await;
    expect_status(response, StatusCode::OK).await;
}

#[sqlx::test]
async fn authentication_lifetimes_come_from_the_loaded_policy(pool: PgPool) {
    let app = spawn_app_with_config(&pool, |config| {
        config.limits.auth.user_token_seconds = 61;
        config.limits.auth.admin_token_seconds = 121;
        config.limits.auth.verification_token_seconds = 181;
        config.limits.auth.reset_token_seconds = 61;
    })
    .await;
    let email = unique_email("lifetime");
    let before = Utc::now().timestamp();
    let response = send(
        &app.app,
        Request::builder()
            .method("POST")
            .uri("/api/v1/auth/register")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"email":email,"password":"GoodPassword123!"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    let body: Value = read_json(expect_status(response, StatusCode::OK).await).await;
    let user = uuid::Uuid::parse_str(body["user"]["id"].as_str().unwrap()).unwrap();
    let mut validation = Validation::default();
    validation.validate_aud = false;
    let claims = decode::<Value>(
        body["token"].as_str().unwrap(),
        &DecodingKey::from_secret(b"test-jwt-secret"),
        &validation,
    )
    .unwrap()
    .claims;
    assert!((61..=71).contains(&(claims["exp"].as_i64().unwrap() - before)));
    let expiry: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT email_verification_expires_at FROM users WHERE id=$1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!((181..=191).contains(&(expiry.timestamp() - before)));

    let before = Utc::now().timestamp();
    let response = send(
        &app.app,
        Request::builder()
            .method("POST")
            .uri("/api/v1/auth/forgot-password")
            .header("content-type", "application/json")
            .body(Body::from(json!({"email":email}).to_string()))
            .unwrap(),
    )
    .await;
    expect_status(response, StatusCode::OK).await;
    let expiry: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT expires_at FROM password_reset_tokens WHERE user_id=$1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!((61..=71).contains(&(expiry.timestamp() - before)));

    let before = Utc::now().timestamp();
    let response = send(
        &app.app,
        Request::builder()
            .method("POST")
            .uri("/api/v1/admin/login")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"email":app.admin_email,"password":app.admin_password}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    let body: Value = read_json(expect_status(response, StatusCode::OK).await).await;
    let claims = decode::<Value>(
        body["token"].as_str().unwrap(),
        &DecodingKey::from_secret(b"test-admin-jwt-secret"),
        &validation,
    )
    .unwrap()
    .claims;
    assert!((121..=131).contains(&(claims["exp"].as_i64().unwrap() - before)));
}
