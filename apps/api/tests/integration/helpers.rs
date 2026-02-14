use api::{
    config::Config,
    infrastructure::{
        database::pool::create_pool,
        ml::traits::{MlService, StyleClassification, TextDetectionResult},
        queue::redis_queue::RedisQueue,
        repositories::{
            sqlx_lettering_repository::SqlxLetteringRepository,
            sqlx_social_repository::SqlxSocialRepository,
        },
        security::virus_scanner::VirusScanner,
        storage::traits::StorageService,
    },
    presentation::http::{routes::create_router, state::AppState},
};
use async_trait::async_trait;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde::de::DeserializeOwned;
use std::{io::Cursor, sync::Arc};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;

#[derive(Clone)]
struct TestStorage;

#[async_trait]
impl StorageService for TestStorage {
    async fn upload(
        &self,
        key: &str,
        _data: Vec<u8>,
        _content_type: &str,
    ) -> anyhow::Result<String> {
        Ok(format!("https://test-storage.local/{}", key))
    }

    async fn delete(&self, _key: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_url(&self, key: &str) -> String {
        format!("https://test-storage.local/{}", key)
    }
}

#[derive(Clone)]
struct TestMlService;

#[async_trait]
impl MlService for TestMlService {
    async fn detect_text(&self, _image_data: &[u8]) -> anyhow::Result<TextDetectionResult> {
        Ok(TextDetectionResult {
            detected_text: "Street Discovery".to_string(),
            confidence: 0.9,
            language: Some("en".to_string()),
        })
    }

    async fn classify_style(&self, _image_data: &[u8]) -> anyhow::Result<StyleClassification> {
        Ok(StyleClassification {
            style: "Hand-painted".to_string(),
            confidence: 0.8,
        })
    }

    async fn extract_colors(&self, _image_data: &[u8]) -> anyhow::Result<Vec<String>> {
        Ok(vec!["#000000".to_string(), "#ffffff".to_string()])
    }
}

pub struct TestApp {
    pub app: Router,
    pub admin_email: String,
    pub admin_password: String,
}

fn build_config(admin_password_hash: String, database_url: String) -> Config {
    Config {
        database_url,
        database_max_connections: 5,
        redis_url: std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
        r2_access_key_id: "test".to_string(),
        r2_secret_access_key: "test".to_string(),
        r2_endpoint: "https://test.r2.cloudflarestorage.com".to_string(),
        r2_region: "auto".to_string(),
        r2_force_path_style: false,
        r2_bucket_name: "test".to_string(),
        r2_public_url: "https://test.r2.dev".to_string(),
        host: "127.0.0.1".to_string(),
        port: 0,
        jwt_secret: "test-jwt-secret".to_string(),
        admin_email: "admin@example.com".to_string(),
        admin_password_hash,
        city_discovery_user_agent: None,
        huggingface_token: None,
        enable_ml_processing: false,
        ml_model_path: "./models/text_detector.onnx".to_string(),
        enable_virus_scan: false,
        rate_limit_uploads_per_ip: 1000,
        enable_pending_auto_approve: false,
        pending_auto_approve_minutes: 30,
        pending_auto_approve_interval_seconds: 300,
        pending_auto_approve_batch_size: 50,
        ignore_missing_migrations: true,
    }
}

async fn resolve_database_url() -> String {
    if let Ok(explicit) = std::env::var("DATABASE_URL") {
        return explicit;
    }

    let candidates = [
        "postgresql://dev:dev@127.0.0.1:5432/through-your-letters",
        "postgresql://dev:dev@127.0.0.1:55432/through-your-letters",
        "postgresql://test:test@127.0.0.1:5432/through-your-letters-test",
    ];

    for candidate in candidates {
        if create_pool(candidate, 1).await.is_ok() {
            return candidate.to_string();
        }
    }

    candidates[0].to_string()
}

pub async fn spawn_app() -> TestApp {
    let admin_password = "AdminPassword123!".to_string();
    let admin_password_hash =
        bcrypt::hash(&admin_password, bcrypt::DEFAULT_COST).expect("failed to hash admin password");
    let database_url = resolve_database_url().await;
    let config = build_config(admin_password_hash, database_url);

    let db = create_pool(&config.database_url, config.database_max_connections)
        .await
        .expect("failed to create pool");
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(config.ignore_missing_migrations);
    migrator.run(&db).await.expect("migrations failed");

    let redis = redis::Client::open(config.redis_url.clone()).expect("invalid redis url");
    let queue = Arc::new(RedisQueue::new(redis.clone()));
    let (tx, _) = broadcast::channel(100);

    let state = AppState {
        db: db.clone(),
        redis,
        storage: Arc::new(TestStorage),
        ml_detector: Arc::new(TestMlService),
        queue,
        virus_scanner: Arc::new(VirusScanner::new(false, None, None)),
        config: config.clone(),
        lettering_repo: Arc::new(SqlxLetteringRepository::new(db.clone())),
        social_repo: Arc::new(SqlxSocialRepository::new(db)),
        ws_broadcaster: Arc::new(tx),
    };

    TestApp {
        app: create_router(state),
        admin_email: config.admin_email,
        admin_password,
    }
}

pub async fn send(app: &Router, req: Request<Body>) -> axum::response::Response {
    app.clone().oneshot(req).await.expect("request failed")
}

pub async fn read_json<T: DeserializeOwned>(res: axum::response::Response) -> T {
    let bytes = to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("failed to read body");
    serde_json::from_slice(&bytes).expect("failed to parse json")
}

pub async fn read_text(res: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("failed to read body");
    String::from_utf8(bytes.to_vec()).expect("invalid utf8")
}

pub async fn expect_status(
    res: axum::response::Response,
    expected: http::StatusCode,
) -> axum::response::Response {
    let actual = res.status();
    
    if actual == expected {
        return res; 
    }

    let body = read_text(res).await;
    panic!(
        "HTTP status mismatch. Expected {}, got {}. Response body: {}",
        expected, actual, body
    );
}

pub fn unique_email(prefix: &str) -> String {
    format!("{}-{}@example.com", prefix, Uuid::now_v7())
}

pub fn tiny_png_bytes() -> Vec<u8> {
    let uuid_bytes = *Uuid::now_v7().as_bytes();
    let raw = vec![
        uuid_bytes[0],
        uuid_bytes[1],
        uuid_bytes[2],
        255,
        uuid_bytes[3],
        uuid_bytes[4],
        uuid_bytes[5],
        255,
        uuid_bytes[6],
        uuid_bytes[7],
        uuid_bytes[8],
        255,
        uuid_bytes[9],
        uuid_bytes[10],
        uuid_bytes[11],
        255,
    ];
    let image = image::RgbaImage::from_raw(2, 2, raw).expect("failed to create image");
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("failed to encode png");
    bytes
}

pub fn multipart_upload_body(
    contributor_tag: &str,
    pin_code: &str,
    description: &str,
    city_id: &str,
    image_bytes: &[u8],
) -> (String, Vec<u8>) {
    let boundary = format!("----ttl-boundary-{}", Uuid::now_v7());
    let mut body = Vec::new();

    let mut push_text = |name: &str, value: &str| {
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    };

    push_text("contributor_tag", contributor_tag);
    push_text("pin_code", pin_code);
    push_text("description", description);
    push_text("city_id", city_id);

    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"image\"; filename=\"image.png\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
    body.extend_from_slice(image_bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

    (boundary, body)
}

pub fn assert_status(status: StatusCode, expected: StatusCode) {
    assert_eq!(status, expected, "expected {}, got {}", expected, status);
}
