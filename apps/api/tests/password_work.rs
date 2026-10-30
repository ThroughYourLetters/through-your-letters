#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use helpers::{expect_status, read_json, send, spawn_app_with_config, unique_email};
use serde_json::{Value, json};
use std::{num::NonZeroUsize, time::Duration};

fn request(path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[sqlx::test]
async fn registration_and_reset_reject_truncated_or_short_unicode_passwords(pool: sqlx::PgPool) {
    let app = spawn_app_with_config(&pool, |config| {
        config.limits.security.register.requests = 100;
    })
    .await;
    for password in ["a".repeat(73), "é".repeat(37), "é".repeat(7)] {
        let email = unique_email("password-limit");
        expect_status(
            send(
                &app.app,
                request(
                    "/api/v1/auth/register",
                    json!({
                        "email": email, "password": password
                    }),
                ),
            )
            .await,
            StatusCode::BAD_REQUEST,
        )
        .await;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email=$1)")
            .bind(email)
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert!(!exists);
        expect_status(
            send(
                &app.app,
                request(
                    "/api/v1/auth/reset-password",
                    json!({
                        "token": "unused-token", "new_password": password
                    }),
                ),
            )
            .await,
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    // Exactly 72 bytes, with eight or more Unicode characters, survives hashing intact.
    let password = "é".repeat(36);
    let email = unique_email("password-boundary");
    expect_status(
        send(
            &app.app,
            request(
                "/api/v1/auth/register",
                json!({
                    "email": email, "password": password
                }),
            ),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    expect_status(
        send(
            &app.app,
            request(
                "/api/v1/auth/login",
                json!({
                    "email": email, "password": password
                }),
            ),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let wrong_last_character = format!("{}a", "é".repeat(35));
    expect_status(
        send(
            &app.app,
            request(
                "/api/v1/auth/login",
                json!({
                    "email": email, "password": wrong_last_character
                }),
            ),
        )
        .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
}

#[sqlx::test]
async fn legacy_long_passwords_still_sign_in_and_confirm_deletion(pool: sqlx::PgPool) {
    let legacy = "LegacyPassword".repeat(8);
    let hash = bcrypt::hash(&legacy, 4).unwrap();
    let app =
        spawn_app_with_config(&pool, |config| config.admin_password_hash = hash.clone()).await;
    let email = unique_email("legacy-password");
    let user = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO users(id,email,password_hash,role) VALUES($1,$2,$3,'USER')")
        .bind(user)
        .bind(&email)
        .bind(&hash)
        .execute(&app.db)
        .await
        .unwrap();
    let response = expect_status(
        send(
            &app.app,
            request(
                "/api/v1/auth/login",
                json!({
                    "email": email, "password": legacy
                }),
            ),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let signed_in: Value = read_json(response).await;
    expect_status(
        send(
            &app.app,
            request(
                "/api/v1/admin/login",
                json!({
                    "email": app.admin_email, "password": legacy
                }),
            ),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let deletion = Request::builder()
        .method("DELETE")
        .uri("/api/v1/me/account")
        .header("content-type", "application/json")
        .header(
            "authorization",
            format!("Bearer {}", signed_in["token"].as_str().unwrap()),
        )
        .body(Body::from(json!({"password": legacy}).to_string()))
        .unwrap();
    expect_status(send(&app.app, deletion).await, StatusCode::NO_CONTENT).await;
}

#[sqlx::test]
async fn excessive_verification_inputs_and_hash_costs_fail_before_bcrypt(pool: sqlx::PgPool) {
    let hash = bcrypt::hash("Password123!", 4).unwrap();
    let app =
        spawn_app_with_config(&pool, |config| config.admin_password_hash = hash.clone()).await;
    let email = unique_email("expensive-hash");
    sqlx::query("INSERT INTO users(id,email,password_hash,role) VALUES($1,$2,$3,'USER')")
        .bind(uuid::Uuid::now_v7())
        .bind(&email)
        .bind(hash.replace("$04$", "$15$"))
        .execute(&app.db)
        .await
        .unwrap();
    let response = tokio::time::timeout(
        Duration::from_secs(2),
        send(
            &app.app,
            request(
                "/api/v1/auth/login",
                json!({
                    "email": email, "password": "Password123!"
                }),
            ),
        ),
    )
    .await
    .expect("bcrypt above the configured cost limit must not execute");
    expect_status(response, StatusCode::INTERNAL_SERVER_ERROR).await;
    expect_status(
        send(
            &app.app,
            request(
                "/api/v1/admin/login",
                json!({
                    "email": app.admin_email, "password": "a".repeat(1025)
                }),
            ),
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
}

#[sqlx::test]
async fn concurrent_password_checks_reject_saturation_and_leave_health_responsive(
    pool: sqlx::PgPool,
) {
    let app = spawn_app_with_config(&pool, |config| {
        config.limits.passwords.workers = NonZeroUsize::new(1).unwrap();
        config.limits.security.login.requests = 100;
    })
    .await;
    let email = unique_email("concurrent-password");
    expect_status(
        send(
            &app.app,
            request(
                "/api/v1/auth/register",
                json!({
                    "email": email, "password": "Password123!"
                }),
            ),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let checks = (0..12).map(|_| {
        send(
            &app.app,
            request(
                "/api/v1/auth/login",
                json!({"email": email, "password": "Password123!"}),
            ),
        )
    });
    let concurrent = futures_util::future::join_all(checks);
    let health = async {
        tokio::task::yield_now().await;
        tokio::time::timeout(
            Duration::from_secs(1),
            send(
                &app.app,
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            ),
        )
        .await
        .expect("password work must not block health")
    };
    let (results, response) = tokio::join!(concurrent, health);
    expect_status(response, StatusCode::OK).await;
    assert!(results.iter().any(|res| res.status() == StatusCode::OK));
    assert!(
        results
            .iter()
            .any(|res| res.status() == StatusCode::SERVICE_UNAVAILABLE)
    );
    for response in results {
        assert!(matches!(
            response.status(),
            StatusCode::OK | StatusCode::SERVICE_UNAVAILABLE
        ));
    }
}

#[sqlx::test]
async fn password_json_limits_apply_to_real_routes_without_content_length(pool: sqlx::PgPool) {
    let app = spawn_app_with_config(&pool, |_| {}).await;
    for path in [
        "/api/v1/auth/register",
        "/api/v1/auth/login",
        "/api/v1/auth/reset-password",
        "/api/v1/admin/login",
    ] {
        let body = json!({"email": app.admin_email, "password": "a".repeat(20_000), "token": "x", "new_password": "x"});
        let stream = futures_util::stream::iter([Ok::<_, std::io::Error>(bytes::Bytes::from(
            body.to_string(),
        ))]);
        let request = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from_stream(stream))
            .unwrap();
        expect_status(send(&app.app, request).await, StatusCode::PAYLOAD_TOO_LARGE).await;
    }
}
