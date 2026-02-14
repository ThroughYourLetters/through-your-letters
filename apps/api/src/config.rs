use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub database_url: String,
    pub database_max_connections: u32,
    pub redis_url: String,
    pub r2_access_key_id: String,
    pub r2_secret_access_key: String,
    pub r2_endpoint: String,
    pub r2_region: String,
    pub r2_force_path_style: bool,
    pub r2_bucket_name: String,
    pub r2_public_url: String,
    pub host: String,
    pub port: u16,
    pub jwt_secret: String,
    pub admin_email: String,
    pub admin_password_hash: String,
    pub city_discovery_user_agent: Option<String>,
    pub huggingface_token: Option<String>,
    pub enable_ml_processing: bool,
    pub ml_model_path: String,
    pub enable_virus_scan: bool,
    pub rate_limit_uploads_per_ip: u32,
    pub enable_pending_auto_approve: bool,
    pub pending_auto_approve_minutes: i64,
    pub pending_auto_approve_interval_seconds: u64,
    pub pending_auto_approve_batch_size: i64,
    pub ignore_missing_migrations: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")?,
            database_max_connections: std::env::var("DATABASE_MAX_CONNECTIONS")
                .unwrap_or("20".into())
                .parse()?,
            redis_url: std::env::var("REDIS_URL")?,
            r2_access_key_id: std::env::var("R2_ACCESS_KEY_ID")?,
            r2_secret_access_key: std::env::var("R2_SECRET_ACCESS_KEY")?,
            r2_endpoint: std::env::var("R2_ENDPOINT")?,
            r2_region: std::env::var("R2_REGION").unwrap_or("auto".into()),
            r2_force_path_style: std::env::var("R2_FORCE_PATH_STYLE")
                .unwrap_or("false".into())
                .parse()?,
            r2_bucket_name: std::env::var("R2_BUCKET_NAME")?,
            r2_public_url: std::env::var("R2_PUBLIC_URL")?,
            host: std::env::var("HOST").unwrap_or("0.0.0.0".into()),
            port: std::env::var("PORT").unwrap_or("3000".into()).parse()?,
            jwt_secret: std::env::var("JWT_SECRET")?,
            admin_email: std::env::var("ADMIN_EMAIL")?,
            admin_password_hash: std::env::var("ADMIN_PASSWORD_HASH")?,
            city_discovery_user_agent: std::env::var("CITY_DISCOVERY_USER_AGENT").ok(),
            huggingface_token: std::env::var("HUGGINGFACE_TOKEN").ok(),
            enable_ml_processing: std::env::var("ENABLE_ML_PROCESSING")
                .unwrap_or("true".into())
                .parse()?,
            ml_model_path: std::env::var("ML_MODEL_PATH")
                .unwrap_or("./models/text_detector.onnx".into()),
            enable_virus_scan: std::env::var("ENABLE_VIRUS_SCAN")
                .unwrap_or("false".into())
                .parse()?,
            rate_limit_uploads_per_ip: std::env::var("RATE_LIMIT_UPLOADS_PER_IP")
                .unwrap_or("100".into())
                .parse()?,
            enable_pending_auto_approve: std::env::var("ENABLE_PENDING_AUTO_APPROVE")
                .unwrap_or("true".into())
                .parse()?,
            pending_auto_approve_minutes: std::env::var("PENDING_AUTO_APPROVE_MINUTES")
                .unwrap_or("30".into())
                .parse()?,
            pending_auto_approve_interval_seconds: std::env::var(
                "PENDING_AUTO_APPROVE_INTERVAL_SECONDS",
            )
            .unwrap_or("300".into())
            .parse()?,
            pending_auto_approve_batch_size: std::env::var("PENDING_AUTO_APPROVE_BATCH_SIZE")
                .unwrap_or("50".into())
                .parse()?,
            ignore_missing_migrations: std::env::var("IGNORE_MISSING_MIGRATIONS")
                .unwrap_or("true".into())
                .parse()?,
        })
    }
}
