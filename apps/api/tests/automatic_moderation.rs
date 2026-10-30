#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;

use api::{
    infrastructure::{
        cache::response_cache::ResponseCache,
        ml::onnx_text_detector::OnnxTextDetector,
        queue::postgres_jobs::{ClaimedJob, PostgresJobs},
    },
    presentation::http::middleware::admin::AdminClaims,
    workers::{ml_processor::MlProcessor, pending_auto_approve::PendingAutoApproveWorker},
};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, Utc};
use helpers::{
    TestApp, expect_status, multipart_upload_body, read_json, send, spawn_app,
    spawn_app_with_config, tiny_png_bytes,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, Notify, broadcast::error::TryRecvError};
use uuid::Uuid;

#[derive(Default)]
struct ImageControl {
    requests: AtomicUsize,
    hold: AtomicBool,
    fail: AtomicBool,
    invalid: AtomicBool,
    received: Notify,
    release: Notify,
}

async fn image(State(control): State<Arc<ImageControl>>) -> Response {
    control.requests.fetch_add(1, Ordering::SeqCst);
    control.received.notify_one();
    if control.hold.swap(false, Ordering::SeqCst) {
        control.release.notified().await;
    }
    if control.fail.load(Ordering::SeqCst) {
        StatusCode::SERVICE_UNAVAILABLE.into_response()
    } else if control.invalid.load(Ordering::SeqCst) {
        b"not an image".as_slice().into_response()
    } else {
        tiny_png_bytes().into_response()
    }
}

struct Fixture {
    app: TestApp,
    processor: Arc<MlProcessor>,
    queue: Arc<PostgresJobs>,
    claimed: Mutex<std::collections::HashMap<Uuid, ClaimedJob>>,
    cache: Arc<ResponseCache>,
    control: Arc<ImageControl>,
    image_url: String,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.control.release.notify_waiters();
        self.server.abort();
    }
}

impl Fixture {
    async fn new(pool: &PgPool) -> Self {
        Self::with_ocr(pool, None).await
    }

    async fn with_ocr(
        pool: &PgPool,
        ocr: Option<api::infrastructure::ml::ocr::OcrService>,
    ) -> Self {
        let app = spawn_app(pool).await;
        let control = Arc::new(ImageControl::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let image_url = format!("http://{}/image", listener.local_addr().unwrap());
        let router = Router::new()
            .route("/image", get(image))
            .with_state(control.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let limits = api::limits::Limits::bundled().unwrap();
        let cache = Arc::new(ResponseCache::new(limits.cache));
        let queue = Arc::new(PostgresJobs::new(pool.clone(), limits.queue));
        let processor = Arc::new(MlProcessor::new(
            pool.clone(),
            Arc::new(
                OnnxTextDetector::new(
                    "unused",
                    false,
                    Arc::new(api::infrastructure::images::ImageProcessor::new(
                        limits.images,
                    )),
                )
                .unwrap(),
            ),
            queue.clone(),
            ocr,
            app.ws_broadcaster.clone(),
            cache.clone(),
        ));
        Self {
            app,
            processor,
            queue,
            claimed: Mutex::new(std::collections::HashMap::new()),
            cache,
            control,
            image_url,
            server,
        }
    }

    async fn upload(&self, status: &str, reviewed: bool) -> Uuid {
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO letterings(id,city_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,status,detected_text,ml_style,moderation_reason,moderated_at,moderated_by,created_at) VALUES($1,(SELECT id FROM cities LIMIT 1),'Fixture',$4,'small','medium','large','560001',$2,'Original text','Original style','Original reason',CASE WHEN $3 THEN NOW() END,CASE WHEN $3 THEN 'moderator' END,NOW()-INTERVAL '2 hours')")
            .bind(id).bind(status).bind(reviewed).bind(&self.image_url).execute(&self.app.db).await.unwrap();
        id
    }

    async fn job(&self, id: Uuid) -> ClaimedJob {
        if let Some(job) = self.claimed.lock().await.get(&id).cloned() {
            return job;
        }
        let mut tx = self.app.db.begin().await.unwrap();
        PostgresJobs::enqueue_in(&mut tx, id, 3).await.unwrap();
        tx.commit().await.unwrap();
        let job = self.queue.claim().await.unwrap().unwrap();
        assert_eq!(job.lettering_id, id);
        self.claimed.lock().await.insert(id, job.clone());
        job
    }

    async fn reject(&self, id: Uuid) {
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &AdminClaims {
                sub: "moderator".into(),
                aud: "admin".into(),
                exp: (Utc::now().timestamp() + 3600) as usize,
            },
            &jsonwebtoken::EncodingKey::from_secret(b"test-admin-jwt-secret"),
        )
        .unwrap();
        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/admin/letterings/{id}/reject"))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(json!({"reason":"Keep hidden"}).to_string()))
            .unwrap();
        expect_status(send(&self.app.app, request).await, StatusCode::NO_CONTENT).await;
    }
}

#[derive(Debug, PartialEq, sqlx::FromRow)]
struct Snapshot {
    status: String,
    detected_text: Option<String>,
    ml_style: Option<String>,
    moderation_reason: Option<String>,
    moderated_by: Option<String>,
    moderated_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}

async fn snapshot(pool: &PgPool, id: Uuid) -> Snapshot {
    sqlx::query_as("SELECT status,detected_text,ml_style,moderation_reason,moderated_by,moderated_at,updated_at FROM letterings WHERE id=$1")
        .bind(id).fetch_one(pool).await.unwrap()
}

#[sqlx::test]
async fn invalid_analysis_image_leaves_the_upload_pending_and_job_unacknowledged(pool: PgPool) {
    let fixture = Fixture::new(&pool).await;
    let id = fixture.upload("PENDING", false).await;
    let job = fixture.job(id).await;
    let before = snapshot(&pool, id).await;
    let mut events = fixture.app.ws_broadcaster.subscribe();
    fixture.control.invalid.store(true, Ordering::SeqCst);
    assert!(
        fixture
            .processor
            .process_job(&reqwest::Client::new(), &job)
            .await
            .is_err()
    );
    assert_eq!(snapshot(&pool, id).await, before);
    let state: (String, Uuid) =
        sqlx::query_as("SELECT state,lease_token FROM analysis_jobs WHERE lettering_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, ("running".into(), job.lease_token));
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}

#[sqlx::test]
async fn late_processing_and_retries_preserve_manual_rejections(pool: PgPool) {
    let fixture = Fixture::new(&pool).await;
    let id = fixture.upload("PENDING", false).await;
    fixture.control.hold.store(true, Ordering::SeqCst);
    let mut events = fixture.app.ws_broadcaster.subscribe();
    let processor = fixture.processor.clone();
    let job = fixture.job(id).await;
    let work =
        tokio::spawn(async move { processor.process_job(&reqwest::Client::new(), &job).await });
    tokio::time::timeout(Duration::from_secs(5), fixture.control.received.notified())
        .await
        .unwrap();
    fixture.reject(id).await;
    let decision = snapshot(&pool, id).await;
    fixture.control.release.notify_one();
    tokio::time::timeout(Duration::from_secs(5), work)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(snapshot(&pool, id).await, decision);
    assert_eq!(decision.status, "REJECTED");
    fixture
        .processor
        .process_job(&reqwest::Client::new(), &fixture.job(id).await)
        .await
        .unwrap();
    assert_eq!(fixture.control.requests.load(Ordering::SeqCst), 1);
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}

#[sqlx::test]
async fn approval_rechecks_a_rejection_committed_while_waiting_for_the_row_lock(pool: PgPool) {
    let fixture = Fixture::new(&pool).await;
    let id = fixture.upload("PENDING", false).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE letterings SET status='REJECTED',moderated_at=NOW(),moderated_by='moderator',moderation_reason='Keep hidden' WHERE id=$1")
        .bind(id).execute(&mut *tx).await.unwrap();
    let processor = fixture.processor.clone();
    let job = fixture.job(id).await;
    let work =
        tokio::spawn(async move { processor.process_job(&reqwest::Client::new(), &job).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND state='active')")
                .fetch_one(&pool).await.unwrap();
            if waiting { break; }
            tokio::task::yield_now().await;
        }
    }).await.unwrap();
    tx.commit().await.unwrap();
    let decision = snapshot(&pool, id).await;
    tokio::time::timeout(Duration::from_secs(5), work)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(snapshot(&pool, id).await, decision);
    assert_eq!(decision.status, "REJECTED");
}

#[sqlx::test]
async fn processing_approves_once_and_preserves_later_review_or_failure(pool: PgPool) {
    let fixture = Fixture::new(&pool).await;
    let id = fixture.upload("PENDING", false).await;
    let mut events = fixture.app.ws_broadcaster.subscribe();
    fixture
        .cache
        .get_or_fetch("probe", 60, || async { Ok(1_u32) })
        .await
        .unwrap();
    fixture
        .processor
        .process_job(&reqwest::Client::new(), &fixture.job(id).await)
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, id).await.status, "APPROVED");
    assert!(snapshot(&pool, id).await.detected_text.is_none());
    assert!(events.try_recv().unwrap().contains(&id.to_string()));
    assert_eq!(
        fixture
            .cache
            .get_or_fetch("probe", 60, || async { Ok(2_u32) })
            .await
            .unwrap(),
        2
    );
    fixture
        .processor
        .process_job(&reqwest::Client::new(), &fixture.job(id).await)
        .await
        .unwrap();
    assert_eq!(fixture.control.requests.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture
            .cache
            .get_or_fetch("probe", 60, || async { Ok(3_u32) })
            .await
            .unwrap(),
        2
    );
    fixture.reject(id).await;
    let decision = snapshot(&pool, id).await;
    fixture
        .processor
        .process_job(&reqwest::Client::new(), &fixture.job(id).await)
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, id).await, decision);
    let pending = fixture.upload("PENDING", false).await;
    let before = snapshot(&pool, pending).await;
    fixture.control.fail.store(true, Ordering::SeqCst);
    assert!(
        fixture
            .processor
            .process_job(&reqwest::Client::new(), &fixture.job(pending).await)
            .await
            .is_err()
    );
    assert_eq!(snapshot(&pool, pending).await, before);
    for (status, reviewed) in [
        ("REJECTED", false),
        ("REPORTED", false),
        ("APPROVED", true),
        ("PENDING", true),
    ] {
        let id = fixture.upload(status, reviewed).await;
        let before = snapshot(&pool, id).await;
        fixture
            .processor
            .process_job(&reqwest::Client::new(), &fixture.job(id).await)
            .await
            .unwrap();
        assert_eq!(snapshot(&pool, id).await, before);
    }
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}

#[sqlx::test]
async fn real_ocr_is_saved_but_provider_failures_keep_jobs_and_uploads_unfinished(pool: PgPool) {
    use api::infrastructure::ml::ocr::OcrService;
    let response = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/ocr", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let failed = response.clone();
    let counter = calls.clone();
    let router = Router::new().route(
        "/ocr",
        axum::routing::post(move || {
            let failed = failed.clone();
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                if failed.load(Ordering::SeqCst) {
                    StatusCode::SERVICE_UNAVAILABLE.into_response()
                } else {
                    axum::Json(json!([{"generated_text":"नमस्ते"}])).into_response()
                }
            }
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let fixture = Fixture::with_ocr(
        &pool,
        Some(OcrService::new(endpoint, "fixture".into(), 1024)),
    )
    .await;
    let id = fixture.upload("PENDING", false).await;
    fixture
        .processor
        .process_job(&reqwest::Client::new(), &fixture.job(id).await)
        .await
        .unwrap();
    assert_eq!(
        snapshot(&pool, id).await.detected_text.as_deref(),
        Some("नमस्ते")
    );
    assert_eq!(snapshot(&pool, id).await.status, "APPROVED");
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT ml_script FROM letterings WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "Devanagari"
    );
    response.store(true, Ordering::SeqCst);
    let pending = fixture.upload("PENDING", false).await;
    let before = snapshot(&pool, pending).await;
    let job = fixture.job(pending).await;
    assert!(
        fixture
            .processor
            .process_job(&reqwest::Client::new(), &job)
            .await
            .is_err()
    );
    assert_eq!(snapshot(&pool, pending).await, before);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM analysis_jobs WHERE lettering_id=$1")
            .bind(pending)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "running"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    server.abort();
}

#[sqlx::test]
async fn age_approval_skips_reviewed_rows_and_concurrent_moderator_locks(pool: PgPool) {
    let fixture = Fixture::new(&pool).await;
    let locked = fixture.upload("PENDING", false).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE letterings SET status='REJECTED',moderated_at=NOW(),moderated_by='moderator' WHERE id=$1").bind(locked).execute(&mut *tx).await.unwrap();
    let mut protected = Vec::new();
    for (status, reviewed) in [
        ("REJECTED", true),
        ("REPORTED", false),
        ("APPROVED", true),
        ("PENDING", true),
    ] {
        let id = fixture.upload(status, reviewed).await;
        protected.push((id, snapshot(&pool, id).await));
    }
    let mut pending = Vec::new();
    for _ in 0..3 {
        pending.push(fixture.upload("PENDING", false).await);
    }
    let worker = PendingAutoApproveWorker::new(
        pool.clone(),
        fixture.app.ws_broadcaster.clone(),
        1,
        10,
        2,
        fixture.cache.clone(),
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), worker.run_once())
            .await
            .unwrap()
            .unwrap(),
        2
    );
    tx.commit().await.unwrap();
    assert_eq!(worker.run_once().await.unwrap(), 1);
    assert_eq!(worker.run_once().await.unwrap(), 0);
    assert_eq!(snapshot(&pool, locked).await.status, "REJECTED");
    for id in pending {
        assert_eq!(snapshot(&pool, id).await.status, "APPROVED");
    }
    for (id, before) in protected {
        assert_eq!(snapshot(&pool, id).await, before);
    }
}

#[sqlx::test]
async fn upload_and_owner_roll_back_if_job_creation_fails(pool: PgPool) {
    let app = spawn_app_with_config(&pool, |config| config.enable_ml_processing = true).await;
    let user = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO users(id,email,password_hash) VALUES($1,'job-owner@example.test','fixture')",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &api::presentation::http::middleware::user::UserClaims {
            sub: user.to_string(),
            email: "job-owner@example.test".into(),
            role: "USER".into(),
            exp: (Utc::now().timestamp() + 3600) as usize,
            session_version: 0,
        },
        &jsonwebtoken::EncodingKey::from_secret(b"test-jwt-secret"),
    )
    .unwrap();
    sqlx::raw_sql("CREATE FUNCTION fail_job() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'simulated job write failure'; END $$; CREATE TRIGGER fail_job BEFORE INSERT ON analysis_jobs FOR EACH ROW EXECUTE FUNCTION fail_job();")
        .execute(&pool).await.unwrap();
    let mut events = app.ws_broadcaster.subscribe();
    let (boundary, body) = multipart_upload_body(
        "Queue fixture",
        "560001",
        "Queue failure",
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
    expect_status(
        send(&app.app, request).await,
        StatusCode::INTERNAL_SERVER_ERROR,
    )
    .await;
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM letterings),(SELECT COUNT(*) FROM analysis_jobs)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0));
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}

#[sqlx::test]
async fn disabled_processing_preserves_review_recorded_before_immediate_approval(pool: PgPool) {
    let app = spawn_app(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION record_review() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.status := 'REJECTED'; NEW.moderated_at := NOW(); NEW.moderated_by := 'moderator'; NEW.moderation_reason := 'Keep hidden'; RETURN NEW; END $$; CREATE TRIGGER record_review BEFORE INSERT ON letterings FOR EACH ROW EXECUTE FUNCTION record_review();")
        .execute(&pool).await.unwrap();
    let mut events = app.ws_broadcaster.subscribe();
    let (boundary, body) = multipart_upload_body(
        "Reviewed fixture",
        "560001",
        "Already reviewed",
        "0194f123-4567-7abc-8def-0123456789ab",
        &tiny_png_bytes(),
    );
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/letterings/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let response = expect_status(send(&app.app, request).await, StatusCode::OK).await;
    let body: Value = read_json(response).await;
    let id = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
    assert_eq!(body["status"], "rejected");
    let decision = snapshot(&pool, id).await;
    assert_eq!(decision.status, "REJECTED");
    assert_eq!(decision.moderation_reason.as_deref(), Some("Keep hidden"));
    assert!(decision.detected_text.is_none());
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}

#[sqlx::test]
async fn expired_worker_cannot_publish_after_a_new_worker_reclaims_the_job(pool: PgPool) {
    let fixture = Fixture::new(&pool).await;
    let id = fixture.upload("PENDING", false).await;
    let old = fixture.job(id).await;
    fixture.control.hold.store(true, Ordering::SeqCst);
    let mut events = fixture.app.ws_broadcaster.subscribe();
    let processor = fixture.processor.clone();
    let work =
        tokio::spawn(async move { processor.process_job(&reqwest::Client::new(), &old).await });
    tokio::time::timeout(Duration::from_secs(5), fixture.control.received.notified())
        .await
        .unwrap();
    sqlx::query("UPDATE analysis_jobs SET lease_expires_at=clock_timestamp()-INTERVAL '1 second' WHERE lettering_id=$1")
        .bind(id).execute(&pool).await.unwrap();
    let current = fixture.queue.claim().await.unwrap().unwrap();
    assert_eq!(current.attempt, 2);
    let before = snapshot(&pool, id).await;
    fixture.control.release.notify_one();
    tokio::time::timeout(Duration::from_secs(5), work)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(snapshot(&pool, id).await, before);
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    let token: Uuid =
        sqlx::query_scalar("SELECT lease_token FROM analysis_jobs WHERE lettering_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(token, current.lease_token);
    fixture
        .processor
        .process_job(&reqwest::Client::new(), &current)
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, id).await.status, "APPROVED");
    assert!(events.try_recv().unwrap().contains(&id.to_string()));
}

#[sqlx::test]
async fn failed_result_write_rolls_back_job_acknowledgement_and_broadcast(pool: PgPool) {
    let fixture = Fixture::new(&pool).await;
    let id = fixture.upload("PENDING", false).await;
    let job = fixture.job(id).await;
    let before = snapshot(&pool, id).await;
    sqlx::raw_sql("CREATE FUNCTION fail_analysis() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'simulated result failure'; END $$; CREATE TRIGGER fail_analysis BEFORE UPDATE OF detected_text ON letterings FOR EACH ROW EXECUTE FUNCTION fail_analysis();")
        .execute(&pool).await.unwrap();
    let mut events = fixture.app.ws_broadcaster.subscribe();
    assert!(
        fixture
            .processor
            .process_job(&reqwest::Client::new(), &job)
            .await
            .is_err()
    );
    assert_eq!(snapshot(&pool, id).await, before);
    let state: (String, Uuid) =
        sqlx::query_as("SELECT state,lease_token FROM analysis_jobs WHERE lettering_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, ("running".into(), job.lease_token));
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}

struct ApiProcess {
    child: std::process::Child,
    directory: std::path::PathBuf,
}

impl Drop for ApiProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn start_worker_process(pool: &PgPool) -> ApiProcess {
    start_worker_process_with_limits(pool, |_| {})
}

fn start_worker_process_with_limits(
    pool: &PgPool,
    configure: impl FnOnce(&mut Value),
) -> ApiProcess {
    use sqlx::ConnectOptions;
    use std::process::{Command, Stdio};
    let directory = std::env::temp_dir().join(format!("ttl-worker-{}", Uuid::now_v7()));
    std::fs::create_dir(&directory).unwrap();
    let mut limits: Value = serde_json::from_str(include_str!("../config/runtime.json")).unwrap();
    configure(&mut limits);
    let limits_path = directory.join("runtime.json");
    std::fs::write(&limits_path, serde_json::to_vec(&limits).unwrap()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_api"));
    command
        .env_clear()
        .current_dir(&directory)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("TTL_LIMITS_FILE", limits_path)
        .envs([
            (
                "DATABASE_URL",
                pool.connect_options().to_url_lossy().to_string(),
            ),
            ("DATABASE_MAX_CONNECTIONS", "5".into()),
            ("REDIS_URL", "redis://127.0.0.1:9".into()),
            ("R2_ACCESS_KEY_ID", "fixture".into()),
            ("R2_SECRET_ACCESS_KEY", "fixture".into()),
            ("R2_ENDPOINT", "http://127.0.0.1:9".into()),
            ("R2_REGION", "auto".into()),
            ("R2_BUCKET_NAME", "fixture".into()),
            ("R2_PUBLIC_URL", "http://localhost".into()),
            ("JWT_SECRET", "fixture".into()),
            ("ADMIN_JWT_SECRET", "fixture-admin".into()),
            ("ADMIN_EMAIL", "admin@example.test".into()),
            ("ADMIN_PASSWORD_HASH", "fixture".into()),
            ("APP_BASE_URL", "http://localhost".into()),
            ("ALLOWED_ORIGINS", "http://localhost".into()),
            ("ENABLE_ML_PROCESSING", "true".into()),
            ("ENABLE_ML_WORKER", "true".into()),
            ("ML_MODEL_PATH", "missing-model.onnx".into()),
            ("ENABLE_PENDING_AUTO_APPROVE", "false".into()),
            ("ENABLE_VIRUS_SCAN", "false".into()),
            ("HOST", "127.0.0.1".into()),
            ("PORT", "0".into()),
        ]);
    for name in ["ORT_DYLIB_PATH", "LD_LIBRARY_PATH"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    ApiProcess {
        child: command.spawn().unwrap(),
        directory,
    }
}

#[cfg(unix)]
fn terminate(process: &ApiProcess) {
    assert!(
        std::process::Command::new("sh")
            .args(["-c", "kill -TERM \"$1\"", "terminate-worker"])
            .arg(process.child.id().to_string())
            .status()
            .unwrap()
            .success()
    );
}

#[cfg(unix)]
async fn expect_clean_exit(process: &mut ApiProcess) {
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            if let Some(status) = process.child.try_wait().unwrap() {
                assert!(status.success(), "API exited with {status}");
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("API did not exit inside its shutdown budget");
}

#[cfg(unix)]
#[sqlx::test]
async fn sigterm_releases_active_analysis_and_a_new_process_resumes_without_lease_expiry(
    pool: PgPool,
) {
    let fixture = Fixture::new(&pool).await;
    let id = fixture.upload("PENDING", false).await;
    let before = snapshot(&pool, id).await;
    let mut tx = pool.begin().await.unwrap();
    PostgresJobs::enqueue_in(&mut tx, id, 3).await.unwrap();
    tx.commit().await.unwrap();
    fixture.control.hold.store(true, Ordering::SeqCst);
    let short_shutdown = |limits: &mut Value| {
        limits["shutdown"] = json!({
            "grace_seconds": 3,
            "release_margin_seconds": 1,
            "runtime_timeout_ms": 100,
        });
    };
    let mut first = start_worker_process_with_limits(&pool, short_shutdown);
    tokio::time::timeout(Duration::from_secs(15), fixture.control.received.notified())
        .await
        .unwrap();
    terminate(&first);
    expect_clean_exit(&mut first).await;
    assert_eq!(snapshot(&pool, id).await, before);
    let interrupted: (String, i32, String, bool) = sqlx::query_as(
        "SELECT state,attempts,last_error_code,lease_token IS NULL AND lease_expires_at IS NULL FROM analysis_jobs WHERE lettering_id=$1"
    ).bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(interrupted, ("ready".into(), 1, "interrupted".into(), true));
    fixture.control.release.notify_waiters();
    sqlx::query("UPDATE analysis_jobs SET available_at=clock_timestamp() WHERE lettering_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let mut second = start_worker_process_with_limits(&pool, short_shutdown);
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let done: bool = sqlx::query_scalar(
                "SELECT state='completed' FROM analysis_jobs WHERE lettering_id=$1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
            if done {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(snapshot(&pool, id).await.status, "APPROVED");
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT attempts FROM analysis_jobs WHERE lettering_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    terminate(&second);
    expect_clean_exit(&mut second).await;
}

#[sqlx::test]
async fn killed_worker_is_recovered_by_a_new_process_while_redis_is_unavailable(pool: PgPool) {
    let fixture = Fixture::new(&pool).await;
    let id = fixture.upload("PENDING", false).await;
    let mut tx = pool.begin().await.unwrap();
    PostgresJobs::enqueue_in(&mut tx, id, 3).await.unwrap();
    tx.commit().await.unwrap();
    fixture.control.hold.store(true, Ordering::SeqCst);
    let first = start_worker_process(&pool);
    tokio::time::timeout(Duration::from_secs(15), fixture.control.received.notified())
        .await
        .unwrap();
    drop(first);
    let running: (String, i32) =
        sqlx::query_as("SELECT state,attempts FROM analysis_jobs WHERE lettering_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(running, ("running".into(), 1));
    sqlx::query("UPDATE analysis_jobs SET lease_expires_at=clock_timestamp()-INTERVAL '1 second' WHERE lettering_id=$1")
        .bind(id).execute(&pool).await.unwrap();
    let _second = start_worker_process(&pool);
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let done: bool = sqlx::query_scalar(
                "SELECT state='completed' FROM analysis_jobs WHERE lettering_id=$1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
            if done {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(snapshot(&pool, id).await.status, "APPROVED");
    let attempts: i32 =
        sqlx::query_scalar("SELECT attempts FROM analysis_jobs WHERE lettering_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempts, 2);
}
