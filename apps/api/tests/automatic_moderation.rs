#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;

use api::{
    infrastructure::{
        cache::response_cache::ResponseCache,
        ml::onnx_text_detector::OnnxTextDetector,
        queue::redis_queue::{MlJob, RedisQueue},
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
    TestApp, expect_status, multipart_upload_body, read_json, send, spawn_app_with_config,
    tiny_png_bytes,
};
use serde_json::{Value, json};
use sqlx::{ConnectOptions, PgPool};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, broadcast::error::TryRecvError};
use uuid::Uuid;

#[derive(Default)]
struct ImageControl {
    requests: AtomicUsize,
    hold: AtomicBool,
    fail: AtomicBool,
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
    } else {
        tiny_png_bytes().into_response()
    }
}

struct Fixture {
    app: TestApp,
    processor: Arc<MlProcessor>,
    cache: Arc<ResponseCache>,
    control: Arc<ImageControl>,
    image_url: String,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl Fixture {
    async fn new(pool: &PgPool) -> Self {
        let url = pool.connect_options().to_url_lossy().to_string();
        let app = spawn_app_with_config(|config| config.database_url = url).await;
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
        let queue = Arc::new(RedisQueue::new(
            redis::Client::open("redis://127.0.0.1:9").unwrap(),
            limits.queue,
        ));
        let processor = Arc::new(MlProcessor::new(
            pool.clone(),
            Arc::new(OnnxTextDetector::new("unused", false).unwrap()),
            queue,
            None,
            app.ws_broadcaster.clone(),
            cache.clone(),
        ));
        Self {
            app,
            processor,
            cache,
            control,
            image_url,
            server,
        }
    }

    async fn upload(&self, status: &str, reviewed: bool) -> Uuid {
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO letterings(id,city_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,status,detected_text,ml_style,moderation_reason,moderated_at,moderated_by,created_at) VALUES($1,(SELECT id FROM cities LIMIT 1),'Fixture','image','small','medium','large','560001',$2,'Original text','Original style','Original reason',CASE WHEN $3 THEN NOW() END,CASE WHEN $3 THEN 'moderator' END,NOW()-INTERVAL '2 hours')")
            .bind(id).bind(status).bind(reviewed).execute(&self.app.db).await.unwrap();
        id
    }

    fn job(&self, id: Uuid) -> MlJob {
        MlJob {
            lettering_id: id,
            image_url: self.image_url.clone(),
            retry_count: 0,
        }
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
async fn late_processing_and_retries_preserve_manual_rejections(pool: PgPool) {
    let fixture = Fixture::new(&pool).await;
    let id = fixture.upload("PENDING", false).await;
    fixture.control.hold.store(true, Ordering::SeqCst);
    let mut events = fixture.app.ws_broadcaster.subscribe();
    let processor = fixture.processor.clone();
    let job = fixture.job(id);
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
        .process_job(&reqwest::Client::new(), &fixture.job(id))
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
    let job = fixture.job(id);
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
        .process_job(&reqwest::Client::new(), &fixture.job(id))
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, id).await.status, "APPROVED");
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
        .process_job(&reqwest::Client::new(), &fixture.job(id))
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
        .process_job(&reqwest::Client::new(), &fixture.job(id))
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, id).await, decision);
    let pending = fixture.upload("PENDING", false).await;
    let before = snapshot(&pool, pending).await;
    fixture.control.fail.store(true, Ordering::SeqCst);
    assert!(
        fixture
            .processor
            .process_job(&reqwest::Client::new(), &fixture.job(pending))
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
            .process_job(&reqwest::Client::new(), &fixture.job(id))
            .await
            .unwrap();
        assert_eq!(snapshot(&pool, id).await, before);
    }
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
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
async fn enqueue_failures_leave_uploads_pending_without_public_events(pool: PgPool) {
    let redis_url = std::env::var("TEST_REDIS_URL").unwrap();
    let client = redis::Client::open(redis_url.as_str()).unwrap();
    let mut connection = client.get_multiplexed_async_connection().await.unwrap();
    let username = format!("ttl-queue-failure-{}", Uuid::now_v7());
    redis::cmd("ACL")
        .arg("SETUSER")
        .arg(&username)
        .arg("on")
        .arg(">fixture-only")
        .arg("~*")
        .arg("+@all")
        .arg("-lpush")
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    let mut restricted = reqwest::Url::parse(&redis_url).unwrap();
    restricted.set_username(&username).unwrap();
    restricted.set_password(Some("fixture-only")).unwrap();
    let url = pool.connect_options().to_url_lossy().to_string();
    let app = spawn_app_with_config(|config| {
        config.database_url = url;
        config.redis_url = restricted.to_string();
        config.enable_ml_processing = true;
    })
    .await;
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
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let response = expect_status(send(&app.app, request).await, StatusCode::OK).await;
    let body: Value = read_json(response).await;
    let id = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
    assert_eq!(body["status"], "pending");
    assert_eq!(snapshot(&pool, id).await.status, "PENDING");
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    let request = Request::builder()
        .uri(format!("/api/v1/letterings/{id}"))
        .body(Body::empty())
        .unwrap();
    expect_status(send(&app.app, request).await, StatusCode::NOT_FOUND).await;
    redis::cmd("ACL")
        .arg("DELUSER")
        .arg(&username)
        .query_async::<i64>(&mut connection)
        .await
        .unwrap();
}

#[sqlx::test]
async fn disabled_processing_preserves_review_recorded_before_immediate_approval(pool: PgPool) {
    let url = pool.connect_options().to_url_lossy().to_string();
    let app = spawn_app_with_config(|config| config.database_url = url).await;
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
