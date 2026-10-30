#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;
#[path = "support/log_capture.rs"]
mod log_capture;

use api::{
    infrastructure::monitoring::HttpMetrics,
    presentation::http::{
        errors::AppError,
        middleware::{
            admin::AdminClaims, logging::logging_middleware, request_id::request_id_middleware,
        },
    },
};
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
    middleware,
    response::IntoResponse,
    routing::{get, post},
};
use helpers::{read_json, send, spawn_app_with_config};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::Value;
use sqlx::{ConnectOptions, PgPool};
use std::{sync::Arc, time::Duration};
use tokio::task::JoinSet;

fn request(method: Method, path: &str, authorization: &str) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if !authorization.is_empty() {
        builder = builder.header("authorization", authorization);
    }
    builder
        .header("content-type", "application/json")
        .body(Body::from("{\"content\":\"private-request-content\"}"))
        .unwrap()
}

#[sqlx::test]
async fn request_metrics_measure_real_outcomes_with_bounded_private_safe_labels(pool: PgPool) {
    let logs = log_capture::capture_logs();
    let metrics = Arc::new(HttpMetrics::default());
    let router = Router::new()
        .route(
            "/records/{id}",
            get(|| async {
                tokio::time::sleep(Duration::from_millis(2)).await;
                StatusCode::NO_CONTENT
            }),
        )
        .route(
            "/failure",
            post(|| async { AppError::Database("private-database-error-detail".into()) }),
        )
        .layer(middleware::from_fn_with_state(
            metrics.clone(),
            logging_middleware,
        ))
        .layer(middleware::from_fn(request_id_middleware));
    let mut tasks = JoinSet::new();
    for index in 0..100 {
        for (method, path, status) in [
            (
                Method::GET,
                format!("/records/private-record-{index}?token=private-query-token"),
                StatusCode::NO_CONTENT,
            ),
            (
                Method::GET,
                format!("/private-unmatched-{index}?token=private-query-token"),
                StatusCode::NOT_FOUND,
            ),
            (
                Method::from_bytes(format!("CUSTOM{index}").as_bytes()).unwrap(),
                format!("/records/private-record-{index}"),
                StatusCode::METHOD_NOT_ALLOWED,
            ),
        ] {
            let app = router.clone();
            tasks.spawn(async move {
                let response = send(
                    &app,
                    request(method, &path, "Bearer private-authorization-token"),
                )
                .await;
                assert_eq!(response.status(), status);
                let id = response
                    .headers()
                    .get("x-request-id")
                    .unwrap()
                    .to_str()
                    .unwrap();
                uuid::Uuid::parse_str(id).unwrap();
            });
        }
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    assert_eq!(
        send(
            &router,
            request(
                Method::POST,
                "/failure",
                "Bearer private-authorization-token"
            )
        )
        .await
        .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.routes.len(), 4);
    assert_eq!(
        snapshot.routes.iter().map(|row| row.requests).sum::<u64>(),
        301
    );
    let records = snapshot
        .routes
        .iter()
        .find(|row| row.route == "/records/{id}" && row.method == "GET")
        .unwrap();
    assert_eq!(records.requests, 100);
    assert_eq!(records.status_class, 2);
    assert!(records.total_latency_ms >= records.max_latency_ms && records.max_latency_ms > 0.0);
    assert!(
        records
            .latency_bucket_counts
            .windows(2)
            .all(|pair| pair[0] <= pair[1])
    );
    let serialized = serde_json::to_string(&snapshot).unwrap();
    assert!(!serialized.contains("private-"));
    assert!(
        snapshot
            .routes
            .iter()
            .all(|row| ["GET", "POST", "OTHER"].contains(&row.method))
    );

    // Error conversion must not print arbitrary source chains or transport URLs.
    let transport = reqwest::Client::new()
        .get("http://[private-provider-secret")
        .build()
        .unwrap_err();
    let _ = AppError::from(transport).into_response();
    let _ = AppError::from(sqlx::Error::Configuration(Box::new(std::io::Error::other(
        "private-database-password",
    ))))
    .into_response();
    let _ = AppError::from(anyhow::anyhow!("private-inference-output")).into_response();

    // The actual app protects its telemetry route and measures dependency failures.
    let url = pool.connect_options().to_url_lossy().to_string();
    let app = spawn_app_with_config(|config| config.database_url = url).await;
    assert_eq!(
        send(
            &app.app,
            request(
                Method::GET,
                "/api/v1/admin/metrics",
                "Bearer invalid-user-token"
            )
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let token = encode(
        &Header::default(),
        &AdminClaims {
            sub: "metrics-admin".into(),
            exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
            aud: "admin".into(),
        },
        &EncodingKey::from_secret(b"test-admin-jwt-secret"),
    )
    .unwrap();
    let authorization = format!("Bearer {token}");
    let user_id = uuid::Uuid::now_v7();
    let email = format!("private-account-{user_id}@example.test");
    sqlx::query("INSERT INTO users(id,email,password_hash) VALUES($1,$2,'test')")
        .bind(user_id)
        .bind(&email)
        .execute(&pool)
        .await
        .unwrap();
    let user_token = encode(
        &Header::default(),
        &api::presentation::http::middleware::user::UserClaims {
            sub: user_id.to_string(),
            email: email.clone(),
            role: "USER".into(),
            exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
            session_version: 0,
        },
        &EncodingKey::from_secret(b"test-jwt-secret"),
    )
    .unwrap();
    let user_authorization = format!("Bearer {user_token}");
    let gallery = send(
        &app.app,
        request(
            Method::GET,
            "/api/v1/letterings?script=private-search-value",
            &user_authorization,
        ),
    )
    .await;
    assert_eq!(gallery.status(), StatusCode::OK);
    let photo = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO letterings(id,city_id,user_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,status,ml_style,ml_confidence) VALUES($1,(SELECT id FROM cities LIMIT 1),$2,'fixture','image','small','medium','large','560001','APPROVED','hand-painted',0.75)")
        .bind(photo).bind(user_id).execute(&pool).await.unwrap();
    let detail = send(
        &app.app,
        request(
            Method::GET,
            &format!("/api/v1/letterings/{photo}"),
            &user_authorization,
        ),
    )
    .await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail: Value = read_json(detail).await;
    assert_eq!(detail["ml_metadata"]["style"], "hand-painted");
    assert!(detail["ml_metadata"]["confidence"].is_null());
    app.db.close().await;
    assert_eq!(
        send(
            &app.app,
            request(
                Method::GET,
                "/api/v1/letterings?style=private-another-search",
                ""
            )
        )
        .await
        .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        send(&app.app, request(Method::GET, "/health", ""))
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let response = send(
        &app.app,
        request(Method::GET, "/api/v1/admin/metrics", &authorization),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let report: Value = read_json(response).await;
    let routes = report["routes"].as_array().unwrap();
    assert!(routes.iter().any(|r| r["route"] == "/api/v1/letterings"
        && r["status_class"] == 5
        && r["requests"] == 1));
    assert!(
        routes
            .iter()
            .any(|r| r["route"] == "/health" && r["status_class"] == 5 && r["requests"] == 1)
    );
    let output = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    for secret in [
        "private-authorization-token",
        "private-query-token",
        "private-request-content",
        "private-record-",
        "private-unmatched-",
        "private-database-error-detail",
        "private-provider-secret",
        "private-database-password",
        "private-inference-output",
        "private-search-value",
        "private-another-search",
        token.as_str(),
        user_token.as_str(),
        email.as_str(),
    ] {
        assert!(
            !output.contains(secret),
            "private fixture entered logs: {secret}"
        );
        assert!(!report.to_string().contains(secret));
    }
    assert!(output.contains("HTTP response completed") && output.contains("category=\"database\""));
}
