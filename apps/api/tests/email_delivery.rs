// Log capture has its own executable so parallel integration cases cannot change callsite interest.
#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;

use api::{infrastructure::email::email_service::EmailService, limits::Limits};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use helpers::{expect_status, read_json, send, spawn_app_with_email, unique_email};
use serde_json::{Value, json};
#[path = "support/log_capture.rs"]
mod log_capture;

fn request(path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[sqlx::test]
async fn delivery_failures_are_visible_in_logs_but_do_not_reveal_accounts_or_credentials(
    pool: sqlx::PgPool,
) {
    let logs = log_capture::capture_logs();
    for (key, expected_error) in [
        (None, "Email delivery disabled"),
        // Invalid header fails before contacting the public provider.
        (
            Some("private-provider-key\n"),
            "Email provider request failed",
        ),
    ] {
        let email_service = EmailService::new(
            key.map(str::to_owned),
            "archive@example.test".into(),
            "https://archive.example.test".into(),
            &Limits::bundled().unwrap().email,
        )
        .unwrap();
        let app = spawn_app_with_email(&pool, Some(email_service)).await;
        logs.0.lock().unwrap().clear();
        let email = unique_email("private-recovery");
        let password = "PrivateRegistrationPassword123!";
        let registered = expect_status(
            send(
                &app.app,
                request(
                    "/api/v1/auth/register",
                    json!({
                        "email": email, "password": password,
                    }),
                ),
            )
            .await,
            StatusCode::OK,
        )
        .await;
        let account: Value = read_json(registered).await;
        assert!(account["token"].is_string());

        let mut responses = Vec::new();
        for target in [email.clone(), unique_email("unknown-recovery")] {
            let response = expect_status(
                send(
                    &app.app,
                    request(
                        "/api/v1/auth/forgot-password",
                        json!({
                            "email": target,
                        }),
                    ),
                )
                .await,
                StatusCode::OK,
            )
            .await;
            let body: Value = read_json(response).await;
            responses.push(body);
        }
        assert_eq!(responses[0], responses[1]);
        assert!(responses[0].get("token").is_none());
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM password_reset_tokens WHERE user_id = $1"
            )
            .bind(uuid::Uuid::parse_str(account["user"]["id"].as_str().unwrap()).unwrap())
            .fetch_one(&app.db)
            .await
            .unwrap(),
            1
        );

        let captured = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
        assert!(captured.contains("Failed to send verification email"));
        assert!(captured.contains("Failed to send password reset email"));
        assert!(captured.contains(expected_error));
        for secret in [
            email.as_str(),
            password,
            "private-provider-key",
            "raw_token",
            "raw_verification_token",
            "?token=",
            "/verify-email/",
        ] {
            assert!(
                !captured.contains(secret),
                "recovery logs exposed private data"
            );
        }
        assert!(!captured.contains(account["token"].as_str().unwrap()));
    }
}
