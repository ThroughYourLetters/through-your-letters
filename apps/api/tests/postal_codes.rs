#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;
#[path = "support/postal_cases.rs"]
mod postal_cases;

use api::infrastructure::database::migrations::run_migrations;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use helpers::{
    expect_status, multipart_upload_body, read_json, send, spawn_app_with_config, tiny_png_bytes,
};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Value, json};
use sqlx::{ConnectOptions, PgPool, migrate::Migrator};
use std::borrow::Cow;
use uuid::Uuid;

#[sqlx::test]
async fn uploads_and_edits_accept_international_postal_codes_without_truncation(pool: PgPool) {
    let url = pool.connect_options().to_url_lossy().to_string();
    let app = spawn_app_with_config(|config| config.database_url = url).await;
    let user_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO users(id,email,password_hash) VALUES($1,'postal@example.test','test')",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    let token = encode(
        &Header::default(),
        &api::presentation::http::middleware::user::UserClaims {
            sub: user_id.to_string(),
            email: "postal@example.test".into(),
            role: "USER".into(),
            exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
            session_version: 0,
        },
        &EncodingKey::from_secret(b"test-jwt-secret"),
    )
    .unwrap();
    let mut saved_id = None;
    let mut accepted = 0;
    for case in postal_cases::cases() {
        let (boundary, body) = multipart_upload_body(
            "Postal Tester",
            &case.input,
            "Postal code fixture",
            "0194f123-4567-7abc-8def-0123456789ab",
            &tiny_png_bytes(),
        );
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/letterings/upload")
            .header("authorization", format!("Bearer {token}"))
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .unwrap();
        let response = send(&app.app, request).await;
        let expected = if case.normalized.is_some() {
            StatusCode::OK
        } else {
            StatusCode::BAD_REQUEST
        };
        let response = expect_status(response, expected).await;
        if let Some(normalized) = &case.normalized {
            let upload: Value = read_json(response).await;
            let id = Uuid::parse_str(upload["id"].as_str().unwrap()).unwrap();
            let stored: String = sqlx::query_scalar("SELECT pin_code FROM letterings WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(&stored, normalized, "{}", case.name);
            saved_id = saved_id.or(Some(id));
            accepted += 1;
        } else {
            let error: Value = read_json(response).await;
            assert!(
                error["error"]
                    .as_str()
                    .unwrap()
                    .contains("Invalid postal code"),
                "{}: {error}",
                case.name
            );
        }
        if let Some(id) = saved_id {
            let before: String = sqlx::query_scalar("SELECT pin_code FROM letterings WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
            let request = Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/me/letterings/{id}"))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"pin_code": case.input}).to_string()))
                .unwrap();
            let response = expect_status(send(&app.app, request).await, expected).await;
            let updated: Value = read_json(response).await;
            let stored: String = sqlx::query_scalar("SELECT pin_code FROM letterings WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
            match case.normalized {
                Some(normalized) => {
                    assert_eq!(updated["pin_code"], normalized, "{}", case.name);
                    assert_eq!(stored, normalized, "{}", case.name);
                }
                None => assert_eq!(stored, before, "{}", case.name),
            }
        }
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM letterings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, accepted);
}

#[sqlx::test(migrations = false)]
async fn widening_postal_codes_preserves_existing_uploads_and_survives_restart(pool: PgPool) {
    let mut previous: Migrator = sqlx::migrate!("./migrations");
    previous.migrations = Cow::Owned(
        previous
            .iter()
            .filter(|migration| migration.version < 20261030000001)
            .cloned()
            .collect(),
    );
    previous.run(&pool).await.unwrap();
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO letterings(id,city_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,status) VALUES($1,(SELECT id FROM cities LIMIT 1),'Legacy','image','small','medium','large','560001','APPROVED')")
        .bind(id).execute(&pool).await.unwrap();
    run_migrations(&pool, false).await.unwrap();
    let legacy: String = sqlx::query_scalar("SELECT pin_code FROM letterings WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(legacy, "560001");
    let width: i32 = sqlx::query_scalar("SELECT character_maximum_length FROM information_schema.columns WHERE table_schema='public' AND table_name='letterings' AND column_name='pin_code'").fetch_one(&pool).await.unwrap();
    assert_eq!(width, 20);
    let code = "𐐀".repeat(20);
    sqlx::query("UPDATE letterings SET pin_code=$1 WHERE id=$2")
        .bind(&code)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE letterings SET pin_code=$1 WHERE id=$2")
            .bind("𐐀".repeat(21))
            .bind(id)
            .execute(&pool)
            .await
            .is_err()
    );
    run_migrations(&pool, false).await.unwrap();
    let stored: String = sqlx::query_scalar("SELECT pin_code FROM letterings WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, code);
}
