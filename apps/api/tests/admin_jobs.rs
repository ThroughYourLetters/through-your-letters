#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;

use api::{
    infrastructure::queue::postgres_jobs::PostgresJobs, limits::Limits,
    presentation::http::middleware::admin::AdminClaims,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{DateTime, Utc};
use helpers::{TestApp, expect_status, read_json, send, spawn_app};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn admin_token() -> String {
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &AdminClaims {
            sub: "job-operator".into(),
            aud: "admin".into(),
            exp: (Utc::now().timestamp() + 3600) as usize,
        },
        &jsonwebtoken::EncodingKey::from_secret(b"test-admin-jwt-secret"),
    )
    .unwrap()
}

fn request(method: &str, path: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    request.body(Body::from(body.to_string())).unwrap()
}

async fn get(app: &TestApp, path: &str) -> Value {
    read_json(
        expect_status(
            send(
                &app.app,
                request("GET", path, Some(&admin_token()), Value::Null),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await
}

async fn failed_analysis(pool: &PgPool) -> (Uuid, Value) {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO letterings(id,city_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,status) SELECT $1,id,'Job fixture','https://private.example/image?secret=fixture','fixture-thumbnail','fixture-thumbnail','fixture-thumbnail','560001','PENDING' FROM cities LIMIT 1")
        .bind(id).execute(pool).await.unwrap();
    let finished: DateTime<Utc> = sqlx::query_scalar("INSERT INTO analysis_jobs(lettering_id,state,attempts,max_attempts,last_error_code,finished_at) VALUES($1,'failed',3,3,'interrupted',clock_timestamp()) RETURNING finished_at")
        .bind(id).fetch_one(pool).await.unwrap();
    (id, json!({"finished_at":finished}))
}

async fn failed_cleanup(pool: &PgPool, state: &str, error: &str) -> (Uuid, Value) {
    let (id, available_at): (Uuid, DateTime<Utc>) = sqlx::query_as("INSERT INTO storage_cleanup(object_url,state,attempts,last_error_code,available_at) VALUES($1,$2,3,$3,clock_timestamp()+INTERVAL '1 hour') RETURNING id,available_at")
        .bind(format!("https://foreign.example/{}.webp?secret=fixture", Uuid::now_v7()))
        .bind(state).bind(error).fetch_one(pool).await.unwrap();
    (id, json!({"available_at":available_at,"attempts":3}))
}

async fn audit_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM admin_audit_logs WHERE action IN('RETRY_ANALYSIS_JOB','RETRY_STORAGE_CLEANUP')")
        .fetch_one(pool).await.unwrap()
}

#[sqlx::test]
async fn job_inspection_requires_admin_and_returns_bounded_private_safe_pages(pool: PgPool) {
    let app = spawn_app(&pool).await;
    let mut analysis_ids = Vec::new();
    let mut cleanup_ids = Vec::new();
    for _ in 0..3 {
        analysis_ids.push(failed_analysis(&pool).await.0);
        cleanup_ids.push(failed_cleanup(&pool, "failed", "unrecognized_url").await.0);
    }
    cleanup_ids.push(
        failed_cleanup(&pool, "ready", "storage_unavailable")
            .await
            .0,
    );
    analysis_ids.sort();
    cleanup_ids.sort();
    let other_audience = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &json!({
            "sub":"user", "aud":"user", "exp":Utc::now().timestamp()+3600,
        }),
        &jsonwebtoken::EncodingKey::from_secret(b"test-admin-jwt-secret"),
    )
    .unwrap();
    for token in [None, Some(other_audience.as_str()), Some("invalid")] {
        for path in ["/api/v1/admin/jobs/analysis", "/api/v1/admin/jobs/cleanup"] {
            expect_status(
                send(&app.app, request("GET", path, token, Value::Null)).await,
                StatusCode::UNAUTHORIZED,
            )
            .await;
            expect_status(
                send(
                    &app.app,
                    request(
                        "POST",
                        &format!("{path}/{}/retry", Uuid::now_v7()),
                        token,
                        json!({}),
                    ),
                )
                .await,
                StatusCode::UNAUTHORIZED,
            )
            .await;
        }
    }
    for (kind, expected) in [("analysis", analysis_ids), ("cleanup", cleanup_ids)] {
        let mut after = String::new();
        let mut actual = Vec::new();
        loop {
            let page = get(&app, &format!("/api/v1/admin/jobs/{kind}?limit=1{after}")).await;
            assert_eq!(page["items"].as_array().unwrap().len(), 1);
            actual.push(Uuid::parse_str(page["items"][0]["id"].as_str().unwrap()).unwrap());
            assert!(!page.to_string().contains("secret=fixture"));
            assert!(!page.to_string().contains("lease_token"));
            assert!(!page.to_string().contains("object_url"));
            if page["next_cursor"].is_null() {
                break;
            }
            after = format!("&after={}", page["next_cursor"].as_str().unwrap());
        }
        assert_eq!(actual, expected);
        for query in [
            "limit=0",
            "limit=101",
            "limit=-1",
            "limit=18446744073709551615",
            "after=bad-id",
        ] {
            expect_status(
                send(
                    &app.app,
                    request(
                        "GET",
                        &format!("/api/v1/admin/jobs/{kind}?{query}"),
                        Some(&admin_token()),
                        Value::Null,
                    ),
                )
                .await,
                StatusCode::BAD_REQUEST,
            )
            .await;
        }
    }
}

#[sqlx::test]
async fn concurrent_analysis_retries_commit_one_fresh_budget_audit_and_notification(pool: PgPool) {
    let app = spawn_app(&pool).await;
    let (id, body) = failed_analysis(&pool).await;
    let queue = PostgresJobs::new(pool.clone(), Limits::bundled().unwrap().queue);
    let mut listener = queue.listener().await.unwrap();
    let responses = futures_util::future::join_all((0..16).map(|_| {
        send(
            &app.app,
            request(
                "POST",
                &format!("/api/v1/admin/jobs/analysis/{id}/retry"),
                Some(&admin_token()),
                body.clone(),
            ),
        )
    }))
    .await;
    assert_eq!(
        responses
            .iter()
            .filter(|response| response.status() == StatusCode::NO_CONTENT)
            .count(),
        1
    );
    assert_eq!(
        responses
            .iter()
            .filter(|response| response.status() == StatusCode::CONFLICT)
            .count(),
        15
    );
    tokio::time::timeout(std::time::Duration::from_secs(2), listener.recv())
        .await
        .unwrap()
        .unwrap();
    let row: (String,i32,i32,bool) = sqlx::query_as("SELECT state,attempts,max_attempts,last_error_code IS NULL AND finished_at IS NULL AND lease_token IS NULL AND lease_expires_at IS NULL FROM analysis_jobs WHERE lettering_id=$1")
        .bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(row, ("ready".into(), 0, 3, true));
    assert_eq!(audit_count(&pool).await, 1);
    let audit: Value =
        sqlx::query_scalar("SELECT metadata FROM admin_audit_logs WHERE lettering_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audit["previous_attempts"], 3);
    assert_eq!(audit["previous_error"], "interrupted");
    assert_eq!(queue.claim().await.unwrap().unwrap().attempt, 1);
    assert!(
        get(&app, "/api/v1/admin/jobs/analysis").await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test]
async fn reviewed_deleted_active_and_changed_failures_cannot_be_retried(pool: PgPool) {
    let app = spawn_app(&pool).await;
    for status in ["APPROVED", "REJECTED", "REPORTED", "PENDING"] {
        let (id, body) = failed_analysis(&pool).await;
        sqlx::query("UPDATE letterings SET status=$2,moderated_at=clock_timestamp() WHERE id=$1")
            .bind(id)
            .bind(status)
            .execute(&pool)
            .await
            .unwrap();
        let page = get(&app, "/api/v1/admin/jobs/analysis").await;
        assert_eq!(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["id"] == id.to_string())
                .unwrap()["can_retry"],
            false
        );
        expect_status(
            send(
                &app.app,
                request(
                    "POST",
                    &format!("/api/v1/admin/jobs/analysis/{id}/retry"),
                    Some(&admin_token()),
                    body,
                ),
            )
            .await,
            StatusCode::CONFLICT,
        )
        .await;
    }
    let (id, body) = failed_analysis(&pool).await;
    sqlx::query("UPDATE analysis_jobs SET finished_at=finished_at+INTERVAL '1 second' WHERE lettering_id=$1").bind(id).execute(&pool).await.unwrap();
    expect_status(
        send(
            &app.app,
            request(
                "POST",
                &format!("/api/v1/admin/jobs/analysis/{id}/retry"),
                Some(&admin_token()),
                body.clone(),
            ),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    sqlx::query("DELETE FROM letterings WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    expect_status(
        send(
            &app.app,
            request(
                "POST",
                &format!("/api/v1/admin/jobs/analysis/{id}/retry"),
                Some(&admin_token()),
                body,
            ),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    assert_eq!(audit_count(&pool).await, 0);
}

#[sqlx::test]
async fn failed_audits_roll_back_both_retry_types_and_emit_no_notification(pool: PgPool) {
    let app = spawn_app(&pool).await;
    let (analysis_id, analysis_body) = failed_analysis(&pool).await;
    let (cleanup_id, cleanup_body) = failed_cleanup(&pool, "failed", "unrecognized_url").await;
    let queue = PostgresJobs::new(pool.clone(), Limits::bundled().unwrap().queue);
    let mut listener = queue.listener().await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION fail_job_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'private audit failure'; END $$; CREATE TRIGGER fail_job_audit BEFORE INSERT ON admin_audit_logs FOR EACH ROW EXECUTE FUNCTION fail_job_audit();")
        .execute(&pool).await.unwrap();
    for (kind, id, body) in [
        ("analysis", analysis_id, analysis_body),
        ("cleanup", cleanup_id, cleanup_body),
    ] {
        let response = expect_status(
            send(
                &app.app,
                request(
                    "POST",
                    &format!("/api/v1/admin/jobs/{kind}/{id}/retry"),
                    Some(&admin_token()),
                    body,
                ),
            )
            .await,
            StatusCode::INTERNAL_SERVER_ERROR,
        )
        .await;
        assert!(
            !read_json::<Value>(response)
                .await
                .to_string()
                .contains("private audit failure")
        );
    }
    assert_eq!(audit_count(&pool).await, 0);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM analysis_jobs WHERE lettering_id=$1")
            .bind(analysis_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "failed"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM storage_cleanup WHERE id=$1")
            .bind(cleanup_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "failed"
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), listener.recv())
            .await
            .is_err()
    );
    sqlx::query("DROP TRIGGER fail_job_audit ON admin_audit_logs")
        .execute(&pool)
        .await
        .unwrap();
    expect_status(
        send(
            &app.app,
            request(
                "POST",
                &format!("/api/v1/admin/jobs/analysis/{analysis_id}/retry"),
                Some(&admin_token()),
                json!({}),
            ),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
}

#[sqlx::test]
async fn unfinished_and_completed_analysis_jobs_never_get_a_manual_retry_budget(pool: PgPool) {
    let app = spawn_app(&pool).await;
    for state in ["ready", "running", "completed"] {
        let (id, body) = failed_analysis(&pool).await;
        sqlx::query("UPDATE analysis_jobs SET state=$2,attempts=CASE WHEN $2='ready' THEN 0 ELSE 1 END,
            finished_at=CASE WHEN $2='completed' THEN clock_timestamp() END,
            lease_token=CASE WHEN $2='running' THEN gen_random_uuid() END,
            lease_expires_at=CASE WHEN $2='running' THEN clock_timestamp()+INTERVAL '1 hour' END WHERE lettering_id=$1")
            .bind(id).bind(state).execute(&pool).await.unwrap();
        expect_status(
            send(
                &app.app,
                request(
                    "POST",
                    &format!("/api/v1/admin/jobs/analysis/{id}/retry"),
                    Some(&admin_token()),
                    body,
                ),
            )
            .await,
            StatusCode::CONFLICT,
        )
        .await;
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM analysis_jobs WHERE lettering_id=$1"
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            state
        );
    }
    assert_eq!(audit_count(&pool).await, 0);
}

#[sqlx::test]
async fn cleanup_retries_preserve_attempt_history_and_cannot_steal_an_active_lease(pool: PgPool) {
    use api::infrastructure::storage::{cleanup::StorageCleanup, traits::StorageService};
    use async_trait::async_trait;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Objects(AtomicUsize);
    #[async_trait]
    impl StorageService for Objects {
        async fn upload(&self, _: &str, _: Vec<u8>, _: &str) -> anyhow::Result<String> {
            unreachable!()
        }
        async fn delete(&self, _: &str) -> anyhow::Result<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn get_url(&self, key: &str) -> String {
            format!("https://owned.example/{key}")
        }
    }
    let app = spawn_app(&pool).await;
    let (id, body) = failed_cleanup(&pool, "ready", "storage_unavailable").await;
    let objects = Arc::new(Objects(AtomicUsize::new(0)));
    let cleaner = StorageCleanup::new(
        pool.clone(),
        objects.clone(),
        Limits::bundled().unwrap().storage,
    );
    let responses = futures_util::future::join_all((0..16).map(|_| {
        send(
            &app.app,
            request(
                "POST",
                &format!("/api/v1/admin/jobs/cleanup/{id}/retry"),
                Some(&admin_token()),
                body.clone(),
            ),
        )
    }))
    .await;
    assert_eq!(
        responses
            .iter()
            .filter(|response| response.status() == StatusCode::NO_CONTENT)
            .count(),
        1
    );
    assert_eq!(
        responses
            .iter()
            .filter(|response| response.status() == StatusCode::CONFLICT)
            .count(),
        15
    );
    assert_eq!(audit_count(&pool).await, 1);
    let claim = cleaner.claim().await.unwrap().unwrap();
    assert_eq!(claim.attempts, 4);
    expect_status(
        send(
            &app.app,
            request(
                "POST",
                &format!("/api/v1/admin/jobs/cleanup/{id}/retry"),
                Some(&admin_token()),
                body,
            ),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    assert!(
        get(&app, "/api/v1/admin/jobs/cleanup").await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(cleaner.release(&claim).await.unwrap());
    assert!(cleaner.run_once().await.unwrap());
    assert_eq!(
        objects.0.load(Ordering::SeqCst),
        0,
        "retry must preserve the storage URL safety check"
    );
    let retained: (String, String) =
        sqlx::query_as("SELECT state,last_error_code FROM storage_cleanup WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(retained, ("failed".into(), "unrecognized_url".into()));
}
