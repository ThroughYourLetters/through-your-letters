#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;
#[path = "support/postgres_commit_proxy.rs"]
mod postgres_commit_proxy;

use api::{
    domain::lettering::{
        entity::{Lettering, ThumbnailUrls},
        errors::DomainError,
        repository::{LetteringRepository, NewUpload, UploadAttempt},
    },
    infrastructure::{
        repositories::sqlx_lettering_repository::SqlxLetteringRepository,
        storage::{
            cleanup::StorageCleanup, traits::StorageService,
            upload_attempts::PostgresUploadAttempts,
        },
    },
    limits::Limits,
};
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use helpers::{
    expect_status, multipart_upload_body, read_json, send, spawn_app_with_storage, tiny_png_bytes,
};
use serde_json::Value;
use sqlx::PgPool;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use uuid::Uuid;

#[sqlx::test]
async fn lost_postgres_commit_acknowledgement_preserves_the_upload_and_its_objects(pool: PgPool) {
    use sqlx::ConnectOptions;
    let mut target = pool.connect_options().to_url_lossy();
    let proxy = postgres_commit_proxy::CommitDropProxy::start(
        target.host_str().unwrap().to_owned(),
        target.port().unwrap_or(5432),
    )
    .await;
    target.set_host(Some("127.0.0.1")).unwrap();
    target.set_port(Some(proxy.address.port())).unwrap();
    target.query_pairs_mut().append_pair("sslmode", "disable");
    let objects = Arc::new(Objects::default());
    let app = spawn_app_with_storage(&pool, objects.clone(), |config| {
        config.database_url = target.to_string();
        config.enable_ml_processing = true;
    })
    .await;
    proxy.arm();
    let image = tiny_png_bytes();
    let key = Uuid::now_v7();
    let response: Value = read_json(
        expect_status(
            tokio::time::timeout(
                std::time::Duration::from_secs(20),
                send(&app.app, request(&image, key)),
            )
            .await
            .unwrap(),
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert!(proxy.dropped_commit_acknowledgement());
    for table in [
        "letterings",
        "analysis_jobs",
        "upload_requests",
        "upload_attempts",
    ] {
        assert_eq!(count(&pool, table).await, 1);
    }
    assert_eq!(objects.uploads.load(Ordering::SeqCst), 2);
    let replay: Value =
        read_json(expect_status(send(&app.app, request(&image, key)).await, StatusCode::OK).await)
            .await;
    assert_eq!(replay["id"], response["id"]);
    sqlx::query(
        "UPDATE upload_attempts SET lease_expires_at=clock_timestamp()-INTERVAL '1 second'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let attempts = PostgresUploadAttempts::new(pool.clone(), Limits::bundled().unwrap().storage);
    assert_eq!(attempts.reconcile_once().await.unwrap(), 0);
    assert_eq!(count(&pool, "storage_cleanup").await, 0);
    assert_eq!(objects.files.lock().unwrap().len(), 2);
    assert_eq!(objects.deletes.load(Ordering::SeqCst), 0);
}

#[derive(Default)]
struct Objects {
    files: Mutex<HashMap<String, Vec<u8>>>,
    uploads: AtomicUsize,
    fail_at: AtomicUsize,
    write_before_error: AtomicBool,
    deletes: AtomicUsize,
}

#[async_trait]
impl StorageService for Objects {
    async fn upload(&self, key: &str, data: Vec<u8>, _: &str) -> anyhow::Result<String> {
        let call = self.uploads.fetch_add(1, Ordering::SeqCst) + 1;
        let fail = call == self.fail_at.load(Ordering::SeqCst);
        if !fail || self.write_before_error.load(Ordering::SeqCst) {
            self.files.lock().unwrap().insert(key.into(), data);
        }
        anyhow::ensure!(!fail, "simulated storage response loss");
        Ok(self.get_url(key))
    }
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.deletes.fetch_add(1, Ordering::SeqCst);
        self.files.lock().unwrap().remove(key);
        Ok(())
    }
    fn get_url(&self, key: &str) -> String {
        format!("https://test-storage.local/{key}")
    }
}

fn request(image: &[u8], key: Uuid) -> Request<Body> {
    let (boundary, body) = multipart_upload_body(
        "Attempt fixture",
        "560001",
        "Fixture",
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

async fn count(pool: &PgPool, table: &str) -> i64 {
    assert!(
        [
            "letterings",
            "analysis_jobs",
            "upload_requests",
            "upload_attempts",
            "storage_cleanup"
        ]
        .contains(&table)
    );
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn recover(pool: &PgPool, objects: &Arc<Objects>) {
    sqlx::query("UPDATE upload_attempts SET lease_expires_at=clock_timestamp()-INTERVAL '1 second' WHERE state='uploading'")
        .execute(pool).await.unwrap();
    let attempts = PostgresUploadAttempts::new(pool.clone(), Limits::bundled().unwrap().storage);
    assert_eq!(attempts.reconcile_once().await.unwrap(), 1);
    let cleanup = StorageCleanup::new(
        pool.clone(),
        objects.clone(),
        Limits::bundled().unwrap().storage,
    );
    for _ in 0..2 {
        assert!(cleanup.run_once().await.unwrap());
    }
    assert_eq!(count(pool, "storage_cleanup").await, 0);
}

#[sqlx::test]
async fn failed_attempt_registration_never_starts_a_storage_write(pool: PgPool) {
    let objects = Arc::new(Objects::default());
    let app = spawn_app_with_storage(&pool, objects.clone(), |_| {}).await;
    sqlx::raw_sql("CREATE FUNCTION fail_registration() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'registration unavailable'; END $$; CREATE TRIGGER fail_registration BEFORE INSERT ON upload_attempts FOR EACH ROW EXECUTE FUNCTION fail_registration();")
        .execute(&pool).await.unwrap();
    expect_status(
        send(&app.app, request(&tiny_png_bytes(), Uuid::now_v7())).await,
        StatusCode::INTERNAL_SERVER_ERROR,
    )
    .await;
    assert_eq!(objects.uploads.load(Ordering::SeqCst), 0);
    assert!(objects.files.lock().unwrap().is_empty());
    assert_eq!(count(&pool, "upload_attempts").await, 0);
    assert_eq!(count(&pool, "letterings").await, 0);
}

#[sqlx::test]
async fn an_attached_transaction_can_commit_after_lease_expiry_without_reconciliation_deleting_its_files(
    pool: PgPool,
) {
    let objects = Arc::new(Objects::default());
    let _app = spawn_app_with_storage(&pool, objects.clone(), |_| {}).await;
    let mut limits = Limits::bundled().unwrap().storage;
    limits.upload_lease_seconds = 1;
    let attempts = PostgresUploadAttempts::new(pool.clone(), limits);
    let photo = photo(Uuid::now_v7(), &objects);
    let attempt = attempts
        .begin(photo.id, &photo.image_url, &photo.thumbnail_urls.small)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(
        PostgresUploadAttempts::lock_in(&mut tx, attempt, &photo)
            .await
            .unwrap()
    );
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    assert_eq!(attempts.reconcile_once().await.unwrap(), 0);
    sqlx::query("INSERT INTO letterings(id,city_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code) VALUES($1,$2,'Fixture',$3,$4,$4,$3,'560001')")
        .bind(photo.id).bind(photo.city_id).bind(&photo.image_url).bind(&photo.thumbnail_urls.small).execute(&mut *tx).await.unwrap();
    assert!(
        PostgresUploadAttempts::commit_in(&mut tx, attempt)
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    assert_eq!(attempts.reconcile_once().await.unwrap(), 0);
    assert_eq!(count(&pool, "storage_cleanup").await, 0);
}

#[sqlx::test]
async fn primary_and_thumbnail_failures_recover_objects_before_and_after_lost_storage_responses(
    pool: PgPool,
) {
    let objects = Arc::new(Objects::default());
    let app = spawn_app_with_storage(&pool, objects.clone(), |config| {
        config.enable_ml_processing = true
    })
    .await;
    for (index, (ordinal, written)) in [(1, false), (1, true), (2, false), (2, true)]
        .into_iter()
        .enumerate()
    {
        let image = tiny_png_bytes();
        let key = Uuid::now_v7();
        objects.fail_at.store(
            objects.uploads.load(Ordering::SeqCst) + ordinal,
            Ordering::SeqCst,
        );
        objects.write_before_error.store(written, Ordering::SeqCst);
        expect_status(
            send(&app.app, request(&image, key)).await,
            StatusCode::INTERNAL_SERVER_ERROR,
        )
        .await;
        for table in ["letterings", "analysis_jobs", "upload_requests"] {
            assert_eq!(count(&pool, table).await, index as i64);
        }
        recover(&pool, &objects).await;
        assert_eq!(objects.files.lock().unwrap().len(), index * 2);
        objects.fail_at.store(0, Ordering::SeqCst);
        expect_status(send(&app.app, request(&image, key)).await, StatusCode::OK).await;
        assert_eq!(objects.files.lock().unwrap().len(), (index + 1) * 2);
        assert_eq!(count(&pool, "letterings").await, (index + 1) as i64);
    }
}

#[sqlx::test]
async fn source_job_receipt_and_attempt_commit_failures_leave_recoverable_objects(pool: PgPool) {
    let objects = Arc::new(Objects::default());
    let app = spawn_app_with_storage(&pool, objects.clone(), |config| {
        config.enable_ml_processing = true
    })
    .await;
    for (index, (table, event)) in [
        ("letterings", "INSERT"),
        ("analysis_jobs", "INSERT"),
        ("upload_requests", "INSERT"),
        ("upload_attempts", "UPDATE"),
    ]
    .into_iter()
    .enumerate()
    {
        sqlx::raw_sql(&format!("CREATE FUNCTION fail_attempt_boundary() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'write unavailable'; END $$; CREATE TRIGGER fail_attempt_boundary BEFORE {event} ON {table} FOR EACH ROW EXECUTE FUNCTION fail_attempt_boundary();"))
            .execute(&pool).await.unwrap();
        let image = tiny_png_bytes();
        let key = Uuid::now_v7();
        expect_status(
            send(&app.app, request(&image, key)).await,
            StatusCode::INTERNAL_SERVER_ERROR,
        )
        .await;
        for table in ["letterings", "analysis_jobs", "upload_requests"] {
            assert_eq!(count(&pool, table).await, index as i64);
        }
        sqlx::raw_sql(&format!(
            "DROP TRIGGER fail_attempt_boundary ON {table}; DROP FUNCTION fail_attempt_boundary();"
        ))
        .execute(&pool)
        .await
        .unwrap();
        recover(&pool, &objects).await;
        assert_eq!(objects.files.lock().unwrap().len(), index * 2);
        expect_status(send(&app.app, request(&image, key)).await, StatusCode::OK).await;
        assert_eq!(objects.files.lock().unwrap().len(), (index + 1) * 2);
    }
}

fn photo(id: Uuid, objects: &Objects) -> Lettering {
    let image = objects.get_url(&format!("letterings/{id}.webp"));
    let thumbnail = objects.get_url(&format!("thumbs/{id}.webp"));
    Lettering {
        id,
        city_id: Uuid::parse_str("0194f123-4567-7abc-8def-0123456789ab").unwrap(),
        image_url: image.clone(),
        thumbnail_urls: ThumbnailUrls {
            small: thumbnail.clone(),
            medium: thumbnail,
            large: image,
        },
        ..Default::default()
    }
}

async fn create(
    pool: &PgPool,
    photo: &Lettering,
    attempt: UploadAttempt,
) -> Result<Lettering, DomainError> {
    SqlxLetteringRepository::new(pool.clone())
        .create(NewUpload {
            lettering: photo,
            owner_id: None,
            analysis_attempts: None,
            request: None,
            storage_attempt: Some(attempt),
        })
        .await
}

#[sqlx::test]
async fn expired_attempts_cannot_commit_before_or_after_reconciliation(pool: PgPool) {
    let objects = Arc::new(Objects::default());
    let _app = spawn_app_with_storage(&pool, objects.clone(), |_| {}).await;
    let attempts = PostgresUploadAttempts::new(pool.clone(), Limits::bundled().unwrap().storage);
    let photo = photo(Uuid::now_v7(), &objects);
    let attempt = attempts
        .begin(photo.id, &photo.image_url, &photo.thumbnail_urls.small)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE upload_attempts SET lease_expires_at=clock_timestamp()-INTERVAL '1 second'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        create(&pool, &photo, attempt).await,
        Err(DomainError::Conflict(_))
    ));
    assert_eq!(attempts.reconcile_once().await.unwrap(), 1);
    assert!(matches!(
        create(&pool, &photo, attempt).await,
        Err(DomainError::Conflict(_))
    ));
    assert_eq!(count(&pool, "letterings").await, 0);
}

#[sqlx::test]
async fn committed_attempts_are_excluded_while_late_storage_writes_are_revisited(pool: PgPool) {
    let objects = Arc::new(Objects::default());
    let _app = spawn_app_with_storage(&pool, objects.clone(), |_| {}).await;
    let attempts = PostgresUploadAttempts::new(pool.clone(), Limits::bundled().unwrap().storage);
    let committed = photo(Uuid::now_v7(), &objects);
    let attempt = attempts
        .begin(
            committed.id,
            &committed.image_url,
            &committed.thumbnail_urls.small,
        )
        .await
        .unwrap();
    create(&pool, &committed, attempt).await.unwrap();
    let abandoned = photo(Uuid::now_v7(), &objects);
    attempts
        .begin(
            abandoned.id,
            &abandoned.image_url,
            &abandoned.thumbnail_urls.small,
        )
        .await
        .unwrap();
    recover(&pool, &objects).await;
    let late_key = format!("letterings/{}.webp", abandoned.id);
    objects
        .files
        .lock()
        .unwrap()
        .insert(late_key.clone(), b"late write".to_vec());
    sqlx::query("UPDATE upload_attempts SET next_cleanup_at=clock_timestamp()-INTERVAL '1 second' WHERE state='cleanup'").execute(&pool).await.unwrap();
    assert_eq!(attempts.reconcile_once().await.unwrap(), 1);
    let cleanup = StorageCleanup::new(
        pool.clone(),
        objects.clone(),
        Limits::bundled().unwrap().storage,
    );
    for _ in 0..2 {
        cleanup.run_once().await.unwrap();
    }
    assert!(!objects.files.lock().unwrap().contains_key(&late_key));
    assert_eq!(count(&pool, "letterings").await, 1);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM upload_attempts WHERE id=$1")
            .bind(committed.id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "committed"
    );
}

#[sqlx::test]
async fn competing_reconcilers_skip_active_attempts_and_respect_batch_limits(pool: PgPool) {
    let objects = Arc::new(Objects::default());
    let _app = spawn_app_with_storage(&pool, objects.clone(), |_| {}).await;
    let mut limits = Limits::bundled().unwrap().storage;
    limits.orphan_batch_size = 2;
    let attempts = PostgresUploadAttempts::new(pool.clone(), limits);
    for _ in 0..6 {
        let photo = photo(Uuid::now_v7(), &objects);
        attempts
            .begin(photo.id, &photo.image_url, &photo.thumbnail_urls.small)
            .await
            .unwrap();
    }
    assert_eq!(attempts.reconcile_once().await.unwrap(), 0);
    sqlx::query(
        "UPDATE upload_attempts SET lease_expires_at=clock_timestamp()-INTERVAL '1 second'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let results = futures_util::future::join_all((0..3).map(|_| attempts.reconcile_once())).await;
    assert!(results.iter().all(|result| matches!(result, Ok(2))));
    assert_eq!(count(&pool, "storage_cleanup").await, 12);
    assert_eq!(attempts.reconcile_once().await.unwrap(), 0);
}
