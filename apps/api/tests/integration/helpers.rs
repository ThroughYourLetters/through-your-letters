use api::{
    config::Config,
    infrastructure::{
        cache::response_cache::ResponseCache,
        database::pool::create_pool,
        email::email_service::EmailService,
        geocoding::nominatim::Geocoder,
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
    http::{HeaderValue, Request, StatusCode},
    middleware::{self, Next},
};
use serde::de::DeserializeOwned;
use std::{io::Cursor, net::Ipv6Addr, sync::Arc};
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
    pub db: sqlx::PgPool,
    pub app: Router,
    pub admin_email: String,
    pub admin_password: String,
    geocoding_server: tokio::task::JoinHandle<()>,
}

impl TestApp {
    pub fn stop_geocoder(&self) {
        self.geocoding_server.abort();
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        self.geocoding_server.abort();
    }
}

fn build_config(admin_password_hash: String, database_url: String, redis_url: String) -> Config {
    Config {
        limits: api::limits::Limits::bundled().unwrap(),
        database_url,
        database_max_connections: 5,
        redis_url,
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
        admin_jwt_secret: "test-admin-jwt-secret".to_string(),
        admin_email: "admin@example.com".to_string(),
        admin_password_hash,
        city_discovery_user_agent: None,
        huggingface_token: None,
        enable_ml_processing: false,
        enable_ml_worker: false,
        ml_model_path: "./models/text_detector.onnx".to_string(),
        enable_virus_scan: false,
        rate_limit_uploads_per_ip: 1000,
        enable_pending_auto_approve: false,
        pending_auto_approve_minutes: 30,
        pending_auto_approve_interval_seconds: 300,
        pending_auto_approve_batch_size: 50,
        ignore_missing_migrations: true,
        allowed_origins: vec![],
        resend_api_key: None,
        email_from: "noreply@test.local".to_string(),
        app_base_url: "http://localhost".to_string(),
    }
}

pub async fn spawn_app() -> TestApp {
    spawn_app_with_email(None).await
}

pub async fn spawn_app_with_email(email_service: Option<EmailService>) -> TestApp {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("set TEST_DATABASE_URL to a disposable PostgreSQL/PostGIS database");
    let redis_url =
        std::env::var("TEST_REDIS_URL").expect("set TEST_REDIS_URL to a disposable Redis instance");
    let admin_password = "AdminPassword123!".to_string();
    let admin_password_hash =
        bcrypt::hash(&admin_password, bcrypt::DEFAULT_COST).expect("failed to hash admin password");
    let config = build_config(admin_password_hash, database_url, redis_url);

    let db = create_pool(&config.database_url, config.database_max_connections)
        .await
        .expect("failed to create pool");
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(config.ignore_missing_migrations);
    migrator.run(&db).await.expect("migrations failed");

    let redis = redis::Client::open(config.redis_url.clone()).expect("invalid redis url");
    let queue = Arc::new(RedisQueue::new(redis.clone(), config.limits.queue.clone()));
    let cache = Arc::new(ResponseCache::new(config.limits.cache.clone()));
    let email_service = Arc::new(email_service.unwrap_or_else(|| {
        EmailService::new(
            None,
            config.email_from.clone(),
            config.app_base_url.clone(),
            &config.limits.email,
        )
        .expect("valid test email configuration")
    }));
    let (tx, _) = broadcast::channel(100);

    // Tests must never contact public providers.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut geocoding_limits = config.limits.geocoding.clone();
    geocoding_limits.search_url = format!("http://{}/search", listener.local_addr().unwrap());
    let geocoding_server = tokio::spawn(async move {
        let provider =
            Router::new().fallback(|| async { axum::Json(Vec::<serde_json::Value>::new()) });
        axum::serve(listener, provider).await.unwrap();
    });

    let state = AppState {
        db: db.clone(),
        redis,
        cache,
        geocoder: Geocoder::new(db.clone(), geocoding_limits, "ttl-test").unwrap(),
        storage: Arc::new(TestStorage),
        ml_detector: Arc::new(TestMlService),
        queue,
        virus_scanner: Arc::new(VirusScanner::new(false, None, None)),
        email_service,
        config: config.clone(),
        lettering_repo: Arc::new(SqlxLetteringRepository::new(db.clone())),
        social_repo: Arc::new(SqlxSocialRepository::new(db.clone())),
        ws_broadcaster: Arc::new(tx),
    };

    // Give fixtures independent client rate counters.
    let client_ip = HeaderValue::from_str(&Ipv6Addr::from(Uuid::now_v7().as_u128()).to_string())
        .expect("IPv6 address is a valid header");
    let app = create_router(state).layer(middleware::from_fn(
        move |mut request: Request<Body>, next: Next| {
            request
                .headers_mut()
                .entry("x-forwarded-for")
                .or_insert_with(|| client_ip.clone());
            next.run(request)
        },
    ));

    TestApp {
        db,
        app,
        admin_email: config.admin_email,
        admin_password,
        geocoding_server,
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
