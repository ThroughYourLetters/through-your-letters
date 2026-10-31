#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;

use api::{
    infrastructure::storage::{cleanup::StorageCleanup, traits::StorageService},
    limits::Limits,
    presentation::http::middleware::admin::AdminClaims,
};
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use helpers::{expect_status, read_json, send, spawn_app};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

#[derive(Default)]
struct Objects {
    calls: Mutex<Vec<String>>,
    fail: AtomicBool,
    hang: AtomicBool,
}

#[async_trait]
impl StorageService for Objects {
    async fn upload(&self, _: &str, _: Vec<u8>, _: &str) -> anyhow::Result<String> {
        unreachable!()
    }
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(key.to_owned());
        if self.hang.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        anyhow::ensure!(
            !self.fail.load(Ordering::SeqCst),
            "simulated missing storage response"
        );
        Ok(())
    }
    fn get_url(&self, key: &str) -> String {
        format!("https://test-storage.local/{key}")
    }
}

async fn photo(pool: &PgPool, object: Uuid, owner: Option<Uuid>) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO letterings(id,city_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,user_id,status) VALUES($1,'0194f123-4567-7abc-8def-0123456789ab','Cleanup fixture',$2,$3,$3,$2,'560001',$4,'APPROVED')")
        .bind(id).bind(format!("https://test-storage.local/letterings/{object}.webp"))
        .bind(format!("https://test-storage.local/thumbs/{object}.webp")).bind(owner).execute(pool).await.unwrap();
    id
}

async fn delete(pool: &PgPool, id: Uuid) {
    sqlx::query("DELETE FROM letterings WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    assert!(
        [
            "letterings",
            "storage_cleanup",
            "notifications",
            "admin_audit_logs"
        ]
        .contains(&table)
    );
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

fn worker(pool: &PgPool, objects: &Arc<Objects>) -> StorageCleanup {
    StorageCleanup::new(
        pool.clone(),
        objects.clone(),
        Limits::bundled().unwrap().storage,
    )
}

#[sqlx::test]
async fn shutdown_releases_an_active_cleanup_claim_for_another_worker(pool: PgPool) {
    use api::infrastructure::shutdown::Shutdown;
    use std::time::Duration;
    let _app = spawn_app(&pool).await;
    let objects = Arc::new(Objects::default());
    objects.hang.store(true, Ordering::SeqCst);
    let cleaner = worker(&pool, &objects);
    let id = photo(&pool, Uuid::now_v7(), None).await;
    delete(&pool, id).await;
    let stop = Shutdown::new(Duration::from_secs(2), Duration::from_secs(1));
    let worker_stop = stop.clone();
    let active = tokio::spawn(async move { cleaner.start(worker_stop).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while objects.calls.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    stop.request();
    tokio::time::timeout(Duration::from_secs(5), active)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(count(&pool, "letterings").await, 0);
    assert_eq!(count(&pool, "storage_cleanup").await, 2);
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM storage_cleanup WHERE state='ready' AND lease_token IS NULL AND lease_expires_at IS NULL")
        .fetch_one(&pool).await.unwrap(), 2);
    objects.hang.store(false, Ordering::SeqCst);
    let replacement = worker(&pool, &objects);
    assert!(replacement.run_once().await.unwrap());
    assert!(replacement.run_once().await.unwrap());
    assert_eq!(count(&pool, "storage_cleanup").await, 0);
}

#[sqlx::test]
async fn unresponsive_storage_has_a_deadline_and_retains_both_objects(pool: PgPool) {
    let _app = spawn_app(&pool).await;
    let objects = Arc::new(Objects::default());
    objects.hang.store(true, Ordering::SeqCst);
    let mut limits = Limits::bundled().unwrap().storage;
    limits.cleanup_request_timeout_seconds = 1;
    let cleaner = StorageCleanup::new(pool.clone(), objects, limits);
    let id = photo(&pool, Uuid::now_v7(), None).await;
    delete(&pool, id).await;
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(3), cleaner.run_once())
            .await
            .unwrap()
            .unwrap()
    );
    assert_eq!(count(&pool, "storage_cleanup").await, 2);
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM storage_cleanup WHERE state='ready' AND last_error_code='storage_unavailable'").fetch_one(&pool).await.unwrap(), 1);
}

#[sqlx::test]
async fn owner_deletion_failure_keeps_the_record_and_queues_no_objects(pool: PgPool) {
    use api::presentation::http::middleware::user::UserClaims;
    let app = spawn_app(&pool).await;
    let owner = Uuid::now_v7();
    sqlx::query("INSERT INTO users(id,email,password_hash,role) VALUES($1,'owner@example.test','unused','USER')")
        .bind(owner).execute(&pool).await.unwrap();
    let id = photo(&pool, Uuid::now_v7(), Some(owner)).await;
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &UserClaims {
            sub: owner.to_string(),
            email: "owner@example.test".into(),
            role: "USER".into(),
            exp: (chrono::Utc::now().timestamp() + 3600) as usize,
            session_version: 0,
        },
        &jsonwebtoken::EncodingKey::from_secret(b"test-jwt-secret"),
    )
    .unwrap();
    sqlx::raw_sql("CREATE FUNCTION fail_owner_delete() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'source unavailable'; END $$; CREATE TRIGGER fail_owner_delete BEFORE DELETE ON letterings FOR EACH ROW EXECUTE FUNCTION fail_owner_delete();")
        .execute(&pool).await.unwrap();
    let request = || {
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/letterings/{id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    expect_status(
        send(&app.app, request()).await,
        StatusCode::INTERNAL_SERVER_ERROR,
    )
    .await;
    assert_eq!(count(&pool, "letterings").await, 1);
    assert_eq!(count(&pool, "storage_cleanup").await, 0);
    sqlx::query("DROP TRIGGER fail_owner_delete ON letterings")
        .execute(&pool)
        .await
        .unwrap();
    expect_status(send(&app.app, request()).await, StatusCode::NO_CONTENT).await;
    assert_eq!(count(&pool, "letterings").await, 0);
    assert_eq!(count(&pool, "storage_cleanup").await, 2);
}

#[sqlx::test]
async fn deletion_and_cleanup_records_commit_or_roll_back_together(pool: PgPool) {
    let _app = spawn_app(&pool).await;
    let objects = Arc::new(Objects::default());
    let cleaner = worker(&pool, &objects);
    let id = photo(&pool, Uuid::now_v7(), None).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("DELETE FROM letterings WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(!cleaner.run_once().await.unwrap());
    tx.rollback().await.unwrap();
    assert_eq!(count(&pool, "letterings").await, 1);
    assert_eq!(count(&pool, "storage_cleanup").await, 0);
    assert!(objects.calls.lock().unwrap().is_empty());
    delete(&pool, id).await;
    assert_eq!(count(&pool, "storage_cleanup").await, 2);
    assert!(cleaner.run_once().await.unwrap());
    assert!(cleaner.run_once().await.unwrap());
    assert_eq!(count(&pool, "storage_cleanup").await, 0);
    assert_eq!(objects.calls.lock().unwrap().len(), 2);
}

#[sqlx::test]
async fn storage_failures_retain_work_and_retry_without_restoring_deleted_records(pool: PgPool) {
    let _app = spawn_app(&pool).await;
    let objects = Arc::new(Objects::default());
    objects.fail.store(true, Ordering::SeqCst);
    let cleaner = worker(&pool, &objects);
    let id = photo(&pool, Uuid::now_v7(), None).await;
    delete(&pool, id).await;
    for _ in 0..2 {
        assert!(cleaner.run_once().await.unwrap());
    }
    assert_eq!(count(&pool, "storage_cleanup").await, 2);
    assert_eq!(count(&pool, "letterings").await, 0);
    let errors: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_cleanup WHERE state='ready' AND last_error_code='storage_unavailable' AND attempts=1 AND lease_token IS NULL AND available_at>clock_timestamp()")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(errors, 2);
    assert!(!cleaner.run_once().await.unwrap());
    objects.fail.store(false, Ordering::SeqCst);
    sqlx::query("UPDATE storage_cleanup SET available_at=clock_timestamp()-INTERVAL '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    for _ in 0..2 {
        assert!(cleaner.run_once().await.unwrap());
    }
    assert_eq!(count(&pool, "storage_cleanup").await, 0);
    assert_eq!(objects.calls.lock().unwrap().len(), 4);
}

#[sqlx::test]
async fn shared_image_references_prevent_object_deletion_until_the_last_record_is_removed(
    pool: PgPool,
) {
    let _app = spawn_app(&pool).await;
    let objects = Arc::new(Objects::default());
    let cleaner = worker(&pool, &objects);
    let object = Uuid::now_v7();
    let first = photo(&pool, object, None).await;
    let second = photo(&pool, object, None).await;
    delete(&pool, first).await;
    for _ in 0..2 {
        cleaner.run_once().await.unwrap();
    }
    assert!(objects.calls.lock().unwrap().is_empty());
    assert_eq!(count(&pool, "storage_cleanup").await, 2);
    delete(&pool, second).await;
    for _ in 0..2 {
        cleaner.run_once().await.unwrap();
    }
    assert_eq!(objects.calls.lock().unwrap().len(), 2);
    assert_eq!(count(&pool, "storage_cleanup").await, 0);
}

#[sqlx::test]
async fn competing_workers_and_restarted_leases_cannot_acknowledge_another_claim(pool: PgPool) {
    let _app = spawn_app(&pool).await;
    let objects = Arc::new(Objects::default());
    let cleaner = worker(&pool, &objects);
    for _ in 0..8 {
        let id = photo(&pool, Uuid::now_v7(), None).await;
        delete(&pool, id).await;
    }
    let claims = futures_util::future::join_all((0..32).map(|_| cleaner.claim())).await;
    let jobs = claims
        .into_iter()
        .filter_map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(jobs.len(), 16);
    assert_eq!(
        jobs.iter()
            .map(|job| job.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        16
    );
    let old = &jobs[0];
    sqlx::query("UPDATE storage_cleanup SET lease_expires_at=clock_timestamp()-INTERVAL '1 second' WHERE id=$1")
        .bind(old.id).execute(&pool).await.unwrap();
    let replacement = worker(&pool, &objects).claim().await.unwrap().unwrap();
    assert_eq!(replacement.id, old.id);
    assert_ne!(replacement.lease_token, old.lease_token);
    assert!(!cleaner.complete(old).await.unwrap());
    assert!(!cleaner.release(old).await.unwrap());
    assert!(cleaner.complete(&replacement).await.unwrap());
}

#[sqlx::test]
async fn foreign_or_unrecognized_object_urls_are_retained_without_storage_calls(pool: PgPool) {
    let _app = spawn_app(&pool).await;
    let objects = Arc::new(Objects::default());
    let cleaner = worker(&pool, &objects);
    sqlx::query("INSERT INTO storage_cleanup(object_url) VALUES($1),($2),($3)")
        .bind(format!(
            "https://foreign.example/letterings/{}.webp",
            Uuid::now_v7()
        ))
        .bind("https://test-storage.local/letterings/../private.webp")
        .bind("https://test-storage.local/other/file.webp")
        .execute(&pool)
        .await
        .unwrap();
    for _ in 0..3 {
        cleaner.run_once().await.unwrap();
    }
    assert!(objects.calls.lock().unwrap().is_empty());
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM storage_cleanup WHERE state='failed' AND last_error_code='unrecognized_url' AND lease_token IS NULL").fetch_one(&pool).await.unwrap(), 3);
}

fn admin_request(method: &str, path: &str, body: Value) -> Request<Body> {
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &AdminClaims {
            sub: "cleanup-admin".into(),
            aud: "admin".into(),
            exp: (chrono::Utc::now().timestamp() + 3600) as usize,
        },
        &jsonwebtoken::EncodingKey::from_secret(b"test-admin-jwt-secret"),
    )
    .unwrap();
    Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[sqlx::test]
async fn admin_audit_failure_rolls_back_deletion_notification_and_cleanup(pool: PgPool) {
    let app = spawn_app(&pool).await;
    let owner = Uuid::now_v7();
    sqlx::query("INSERT INTO users(id,email,password_hash,role) VALUES($1,'owner@example.test','unused','USER')")
        .bind(owner).execute(&pool).await.unwrap();
    let id = photo(&pool, Uuid::now_v7(), Some(owner)).await;
    sqlx::raw_sql("CREATE FUNCTION fail_delete_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'audit unavailable'; END $$; CREATE TRIGGER fail_delete_audit BEFORE INSERT ON admin_audit_logs FOR EACH ROW EXECUTE FUNCTION fail_delete_audit();")
        .execute(&pool).await.unwrap();
    expect_status(
        send(
            &app.app,
            admin_request(
                "DELETE",
                &format!("/api/v1/admin/letterings/{id}"),
                Value::Null,
            ),
        )
        .await,
        StatusCode::INTERNAL_SERVER_ERROR,
    )
    .await;
    assert_eq!(count(&pool, "letterings").await, 1);
    for table in ["storage_cleanup", "notifications", "admin_audit_logs"] {
        assert_eq!(count(&pool, table).await, 0);
    }
    sqlx::query("DROP TRIGGER fail_delete_audit ON admin_audit_logs")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION fail_cleanup_insert() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'cleanup unavailable'; END $$; CREATE TRIGGER fail_cleanup_insert BEFORE INSERT ON storage_cleanup FOR EACH ROW EXECUTE FUNCTION fail_cleanup_insert();")
        .execute(&pool).await.unwrap();
    expect_status(
        send(
            &app.app,
            admin_request(
                "DELETE",
                &format!("/api/v1/admin/letterings/{id}"),
                Value::Null,
            ),
        )
        .await,
        StatusCode::INTERNAL_SERVER_ERROR,
    )
    .await;
    assert_eq!(count(&pool, "letterings").await, 1);
    for table in ["storage_cleanup", "notifications", "admin_audit_logs"] {
        assert_eq!(count(&pool, table).await, 0);
    }
    sqlx::query("DROP TRIGGER fail_cleanup_insert ON storage_cleanup")
        .execute(&pool)
        .await
        .unwrap();
    expect_status(
        send(
            &app.app,
            admin_request(
                "DELETE",
                &format!("/api/v1/admin/letterings/{id}"),
                Value::Null,
            ),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(count(&pool, "letterings").await, 0);
    assert_eq!(count(&pool, "storage_cleanup").await, 2);
    assert_eq!(count(&pool, "notifications").await, 1);
    assert_eq!(count(&pool, "admin_audit_logs").await, 1);
}

#[sqlx::test]
async fn bulk_deletion_commits_each_success_without_cleanup_for_failed_rows(pool: PgPool) {
    let app = spawn_app(&pool).await;
    let good = photo(&pool, Uuid::now_v7(), None).await;
    let bad = photo(&pool, Uuid::now_v7(), None).await;
    sqlx::raw_sql(&format!("CREATE FUNCTION fail_one_delete() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.id='{bad}'::uuid THEN RAISE EXCEPTION 'source unavailable'; END IF; RETURN OLD; END $$; CREATE TRIGGER fail_one_delete BEFORE DELETE ON letterings FOR EACH ROW EXECUTE FUNCTION fail_one_delete();"))
        .execute(&pool).await.unwrap();
    let response: Value = read_json(
        expect_status(
            send(
                &app.app,
                admin_request(
                    "POST",
                    "/api/v1/admin/letterings/bulk",
                    json!({"ids":[good,bad],"action":"delete"}),
                ),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(response["processed"], 1);
    assert_eq!(response["failed"], 1);
    assert_eq!(count(&pool, "letterings").await, 1);
    assert_eq!(count(&pool, "storage_cleanup").await, 2);
    assert_eq!(count(&pool, "admin_audit_logs").await, 1);
}
