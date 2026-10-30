#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;

use api::{
    domain::lettering::{
        entity::{Lettering, ThumbnailUrls},
        repository::{LetteringRepository, NewUpload, UploadRequest},
    },
    infrastructure::repositories::sqlx_lettering_repository::SqlxLetteringRepository,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use helpers::{
    expect_status, multipart_upload_body, read_json, send, spawn_app_with_config, tiny_png_bytes,
};
use serde_json::Value;
use std::{num::NonZeroU32, sync::atomic::Ordering};
use uuid::Uuid;

async fn account(pool: &sqlx::PgPool) -> (Uuid, String) {
    use api::presentation::http::middleware::user::UserClaims;
    let id = Uuid::now_v7();
    let email = format!("{id}@example.test");
    sqlx::query("INSERT INTO users(id,email,password_hash,role) VALUES($1,$2,$3,'USER')")
        .bind(id)
        .bind(&email)
        .bind(bcrypt::hash("Password123!", 4).unwrap())
        .execute(pool)
        .await
        .unwrap();
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &UserClaims {
            sub: id.to_string(),
            email,
            role: "USER".into(),
            exp: (chrono::Utc::now().timestamp() + 3600) as usize,
            session_version: 0,
        },
        &jsonwebtoken::EncodingKey::from_secret(b"test-jwt-secret"),
    )
    .unwrap();
    (id, token)
}

fn owned_upload(image: &[u8], key: Uuid, token: &str) -> Request<Body> {
    let mut request = upload(image, key, "Fixture");
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    request
}

#[sqlx::test]
async fn request_keys_are_scoped_to_accounts_and_removed_when_an_account_is_deleted(
    pool: sqlx::PgPool,
) {
    let app = spawn_app_with_config(&pool, |_| {}).await;
    let (first_id, first_token) = account(&pool).await;
    let (second_id, second_token) = account(&pool).await;
    let key = Uuid::now_v7();
    let first_image = tiny_png_bytes();
    let second_image = tiny_png_bytes();
    let first: Value = read_json(
        expect_status(
            send(&app.app, owned_upload(&first_image, key, &first_token)).await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let second: Value = read_json(
        expect_status(
            send(&app.app, owned_upload(&second_image, key, &second_token)).await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_ne!(first["id"], second["id"]);
    expect_status(
        send(&app.app, owned_upload(&second_image, key, &first_token)).await,
        StatusCode::CONFLICT,
    )
    .await;
    expect_status(
        send(&app.app, upload(&tiny_png_bytes(), key, "Anonymous")).await,
        StatusCode::OK,
    )
    .await;
    let deletion = Request::builder()
        .method("DELETE")
        .uri("/api/v1/me/account")
        .header("authorization", format!("Bearer {first_token}"))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"password":"Password123!"}"#))
        .unwrap();
    expect_status(send(&app.app, deletion).await, StatusCode::NO_CONTENT).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM upload_requests WHERE owner_id=$1")
            .bind(first_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM upload_requests WHERE owner_id=$1")
            .bind(second_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    let replay: Value = read_json(
        expect_status(
            send(&app.app, owned_upload(&second_image, key, &second_token)).await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(replay["id"], second["id"]);
    expect_status(
        send(&app.app, owned_upload(&first_image, key, &first_token)).await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
}

fn upload(image: &[u8], key: Uuid, description: &str) -> Request<Body> {
    let (boundary, body) = multipart_upload_body(
        "Retry Tester",
        "560001",
        description,
        "0194f123-4567-7abc-8def-0123456789ab",
        image,
    );
    Request::builder()
        .method("POST")
        .uri("/api/v1/letterings/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header("idempotency-key", key.to_string())
        .body(Body::from(body))
        .unwrap()
}

#[sqlx::test]
async fn lost_success_response_replays_the_original_upload_without_storage_writes(
    pool: sqlx::PgPool,
) {
    let app = spawn_app_with_config(&pool, |config| config.enable_ml_processing = true).await;
    let image = tiny_png_bytes();
    let key = Uuid::now_v7();
    // The client receives no receipt from its first successful attempt.
    drop(
        expect_status(
            send(&app.app, upload(&image, key, "Fixture")).await,
            StatusCode::OK,
        )
        .await,
    );
    let original: Uuid = sqlx::query_scalar("SELECT id FROM letterings")
        .fetch_one(&pool)
        .await
        .unwrap();
    // A fresh API instance must preserve the original analysis policy and receipt.
    let restarted = spawn_app_with_config(&pool, |_| {}).await;
    let replay: Value = read_json(
        expect_status(
            send(&restarted.app, upload(&image, key, "  Fixture  ")).await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(replay["id"], original.to_string());
    assert_eq!(replay["status"], "processing");
    assert!(replay.get("image_hash").is_none());
    assert!(replay.get("owner_id").is_none());
    assert_eq!(app.storage_uploads.load(Ordering::SeqCst), 2);
    assert_eq!(restarted.storage_uploads.load(Ordering::SeqCst), 0);
    for table in ["letterings", "analysis_jobs", "upload_requests"] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
    }
    sqlx::query("UPDATE letterings SET status='REJECTED',moderated_at=NOW(),moderated_by='fixture' WHERE id=$1")
        .bind(original).execute(&pool).await.unwrap();
    let replay: Value = read_json(
        expect_status(
            send(&app.app, upload(&image, key, "Fixture")).await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(replay["id"], original.to_string());
    assert_eq!(replay["status"], "rejected");
    assert_eq!(app.storage_uploads.load(Ordering::SeqCst), 2);
}

#[sqlx::test]
async fn changed_payload_and_deleted_uploads_cannot_reuse_a_request_key(pool: sqlx::PgPool) {
    let app = spawn_app_with_config(&pool, |_| {}).await;
    let image = tiny_png_bytes();
    let key = Uuid::now_v7();
    expect_status(
        send(&app.app, upload(&image, key, "Original")).await,
        StatusCode::OK,
    )
    .await;
    expect_status(
        send(&app.app, upload(&image, key, "Changed")).await,
        StatusCode::CONFLICT,
    )
    .await;
    expect_status(
        send(&app.app, upload(&tiny_png_bytes(), key, "Original")).await,
        StatusCode::CONFLICT,
    )
    .await;
    sqlx::query("DELETE FROM letterings")
        .execute(&pool)
        .await
        .unwrap();
    expect_status(
        send(&app.app, upload(&image, key, "Original")).await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(app.storage_uploads.load(Ordering::SeqCst), 2);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM letterings")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert!(
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT lettering_id FROM upload_requests")
            .fetch_one(&pool)
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test]
async fn request_record_failure_rolls_back_upload_and_job_then_allows_retry(pool: sqlx::PgPool) {
    let app = spawn_app_with_config(&pool, |config| config.enable_ml_processing = true).await;
    let image = tiny_png_bytes();
    let key = Uuid::now_v7();
    sqlx::raw_sql("CREATE FUNCTION fail_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'receipt failure'; END $$; CREATE TRIGGER fail_receipt BEFORE INSERT ON upload_requests FOR EACH ROW EXECUTE FUNCTION fail_receipt();")
        .execute(&pool).await.unwrap();
    expect_status(
        send(&app.app, upload(&image, key, "Fixture")).await,
        StatusCode::INTERNAL_SERVER_ERROR,
    )
    .await;
    for table in ["letterings", "analysis_jobs", "upload_requests"] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
    sqlx::query("DROP TRIGGER fail_receipt ON upload_requests")
        .execute(&pool)
        .await
        .unwrap();
    expect_status(
        send(&app.app, upload(&image, key, "Fixture")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM upload_requests")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}

#[sqlx::test]
async fn competing_request_keys_commit_only_one_upload_and_job(pool: sqlx::PgPool) {
    let app = spawn_app_with_config(&pool, |_| {}).await;
    let key = Uuid::now_v7();
    let hash = "a".repeat(64);
    let attempts = (0..16).map(|_| {
        let repo = SqlxLetteringRepository::new(app.db.clone());
        let hash = hash.clone();
        async move {
            let photo = Lettering {
                id: Uuid::now_v7(),
                city_id: Uuid::parse_str("0194f123-4567-7abc-8def-0123456789ab").unwrap(),
                image_url: "image".into(),
                thumbnail_urls: ThumbnailUrls {
                    small: "small".into(),
                    medium: "medium".into(),
                    large: "large".into(),
                },
                ..Default::default()
            };
            repo.create(NewUpload {
                storage_attempt: None,
                lettering: &photo,
                owner_id: None,
                analysis_attempts: NonZeroU32::new(3),
                request: Some(UploadRequest {
                    key,
                    payload_hash: &hash,
                }),
            })
            .await
        }
    });
    let results = futures_util::future::join_all(attempts).await;
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    for table in ["letterings", "analysis_jobs", "upload_requests"] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
    }
}

#[sqlx::test]
async fn invalid_and_repeated_request_headers_fail_before_storage(pool: sqlx::PgPool) {
    let app = spawn_app_with_config(&pool, |_| {}).await;
    for key in ["invalid", "00000000-0000-0000-0000-000000000000"] {
        let mut request = upload(&tiny_png_bytes(), Uuid::now_v7(), "Fixture");
        request
            .headers_mut()
            .insert("idempotency-key", key.parse().unwrap());
        expect_status(send(&app.app, request).await, StatusCode::BAD_REQUEST).await;
    }
    let mut request = upload(&tiny_png_bytes(), Uuid::now_v7(), "Fixture");
    request.headers_mut().append(
        "idempotency-key",
        Uuid::now_v7().to_string().parse().unwrap(),
    );
    expect_status(send(&app.app, request).await, StatusCode::BAD_REQUEST).await;
    assert_eq!(app.storage_uploads.load(Ordering::SeqCst), 0);
}
