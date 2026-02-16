//! Application configuration loading from environment variables.
//!
//! All configuration is loaded from the environment at startup via standard `std::env::var`.
//! This ensures the application follows the 12-factor app methodology and supports
//! configuration via environment variables in containerized and cloud deployments.
//!
//! # Environment Variables
//!
//! ## Required Variables
//! - `DATABASE_URL`: PostgreSQL connection string
//! - `REDIS_URL`: Redis connection URL
//! - `R2_ACCESS_KEY_ID`: Cloudflare R2 access key
//! - `R2_SECRET_ACCESS_KEY`: Cloudflare R2 secret key
//! - `R2_ENDPOINT`: Cloudflare R2 API endpoint
//! - `R2_BUCKET_NAME`: S3-compatible bucket name
//! - `R2_PUBLIC_URL`: Public URL for R2 objects
//! - `JWT_SECRET`: Secret key for JWT signing
//! - `ADMIN_EMAIL`: Admin user email address
//! - `ADMIN_PASSWORD_HASH`: Bcrypt hash of admin password
//!
//! ## Optional Variables
//! - `RUST_LOG`: Logging level (default: "info,api=debug,tower_http=debug")
//! - `HOST`: Server bind address (default: "0.0.0.0")
//! - `PORT`: Server port (default: 3000)
//! - `DATABASE_MAX_CONNECTIONS`: DB pool size (default: 20)
//! - `R2_REGION`: AWS region (default: "auto")
//! - `R2_FORCE_PATH_STYLE`: Use path-style URLs (default: false)
//! - `CLAMAV_HOST`: ClamAV host for virus scanning
//! - `CLAMAV_PORT`: ClamAV port
//! - `CITY_DISCOVERY_USER_AGENT`: HTTP user agent for city discovery
//! - `HUGGINGFACE_TOKEN`: HuggingFace API token for ML models
//! - `ENABLE_ML_PROCESSING`: Enable ML text detection (default: true)
//! - `ML_MODEL_PATH`: Path to ONNX model (default: "./models/text_detector.onnx")
//! - `ENABLE_VIRUS_SCAN`: Enable ClamAV scanning (default: false)
//! - `RATE_LIMIT_UPLOADS_PER_IP`: Uploads per IP per day (default: 100)
//! - `ENABLE_PENDING_AUTO_APPROVE`: Enable auto approval worker (default: true)
//! - `PENDING_AUTO_APPROVE_MINUTES`: Minutes to wait before auto-approval (default: 30)
//! - `PENDING_AUTO_APPROVE_INTERVAL_SECONDS`: Worker check interval (default: 300)
//! - `PENDING_AUTO_APPROVE_BATCH_SIZE`: Items per approval batch (default: 50)
//! - `IGNORE_MISSING_MIGRATIONS`: Skip missing migrations (default: true)

use serde::Deserialize;

/// Complete server configuration loaded from environment.
///
/// Represents the full configuration state of the application. All fields are populated from
/// environment variables at startup, with sensible defaults provided where appropriate.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// PostgreSQL connection string (e.g., `postgres://user:pass@localhost/db`)
    pub database_url: String,

    /// Maximum number of concurrent database connections (recommended: 20-50)
    pub database_max_connections: u32,

    /// Redis connection URL for queues and caching
    pub redis_url: String,

    /// Cloudflare R2 access key ID
    pub r2_access_key_id: String,

    /// Cloudflare R2 secret access key
    pub r2_secret_access_key: String,

    /// Cloudflare R2 API endpoint (e.g., `https://xxx.r2.cloudflarestorage.com`)
    pub r2_endpoint: String,

    /// AWS region for R2 (typically "auto" or "us-east-1")
    pub r2_region: String,

    /// Use path-style URLs instead of virtual-hosted-style (for S3-compatible services)
    pub r2_force_path_style: bool,

    /// R2 bucket name where images are stored
    pub r2_bucket_name: String,

    /// Public URL for accessing R2 objects (e.g., `https://cdn.example.com`)
    pub r2_public_url: String,

    /// Server bind address
    pub host: String,

    /// Server port
    pub port: u16,

    /// Secret key for JWT token signing and verification
    pub jwt_secret: String,

    /// Admin user email address
    pub admin_email: String,

    /// Bcrypt-hashed admin password (generate with `bcrypt::hash`)
    pub admin_password_hash: String,

    /// HTTP User-Agent for city discovery requests
    pub city_discovery_user_agent: Option<String>,

    /// HuggingFace API token for accessing model hub
    pub huggingface_token: Option<String>,

    /// Enable ML-based text detection in uploaded images
    pub enable_ml_processing: bool,

    /// Path to ONNX model file for text detection
    pub ml_model_path: String,

    /// Enable virus scanning via ClamAV
    pub enable_virus_scan: bool,

    /// Rate limit: maximum uploads per IP address per day
    pub rate_limit_uploads_per_ip: u32,

    /// Enable automatic approval of pending letterings
    pub enable_pending_auto_approve: bool,

    /// Minutes to wait before auto-approving pending items
    pub pending_auto_approve_minutes: i64,

    /// Interval in seconds for auto-approval worker checks
    pub pending_auto_approve_interval_seconds: u64,

    /// Number of items to process per auto-approval batch
    pub pending_auto_approve_batch_size: i64,

    /// Skip missing migrations during startup
    pub ignore_missing_migrations: bool,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error if any required environment variable is missing or
    /// cannot be parsed to the expected type.
    ///
    /// # Defaults
    ///
    /// Several configuration values have sensible defaults and will not error
    /// if the corresponding environment variable is not set.
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: env_required("DATABASE_URL")?,
            database_max_connections: env_or("DATABASE_MAX_CONNECTIONS", 20)?,
            redis_url: env_required("REDIS_URL")?,
            r2_access_key_id: env_required("R2_ACCESS_KEY_ID")?,
            r2_secret_access_key: env_required("R2_SECRET_ACCESS_KEY")?,
            r2_endpoint: env_required("R2_ENDPOINT")?,
            r2_region: env_or("R2_REGION", "auto".to_string())?,
            r2_force_path_style: env_or("R2_FORCE_PATH_STYLE", false)?,
            r2_bucket_name: env_required("R2_BUCKET_NAME")?,
            r2_public_url: env_required("R2_PUBLIC_URL")?,
            host: env_or("HOST", "0.0.0.0".to_string())?,
            port: env_or("PORT", 3000)?,
            jwt_secret: env_required("JWT_SECRET")?,
            admin_email: env_required("ADMIN_EMAIL")?,
            admin_password_hash: env_required("ADMIN_PASSWORD_HASH")?,
            city_discovery_user_agent: std::env::var("CITY_DISCOVERY_USER_AGENT").ok(),
            huggingface_token: std::env::var("HUGGINGFACE_TOKEN").ok(),
            enable_ml_processing: env_or("ENABLE_ML_PROCESSING", true)?,
            ml_model_path: env_or(
                "ML_MODEL_PATH",
                "./models/text_detector.onnx".to_string(),
            )?,
            enable_virus_scan: env_or("ENABLE_VIRUS_SCAN", false)?,
            rate_limit_uploads_per_ip: env_or("RATE_LIMIT_UPLOADS_PER_IP", 100)?,
            enable_pending_auto_approve: env_or("ENABLE_PENDING_AUTO_APPROVE", true)?,
            pending_auto_approve_minutes: env_or("PENDING_AUTO_APPROVE_MINUTES", 30)?,
            pending_auto_approve_interval_seconds: env_or(
                "PENDING_AUTO_APPROVE_INTERVAL_SECONDS",
                300,
            )?,
            pending_auto_approve_batch_size: env_or("PENDING_AUTO_APPROVE_BATCH_SIZE", 50)?,
            ignore_missing_migrations: env_or("IGNORE_MISSING_MIGRATIONS", true)?,
        })
    }
}

/// Load a required environment variable.
///
/// # Errors
///
/// Returns an error if the variable is not set.
fn env_required(key: &str) -> anyhow::Result<String> {
    std::env::var(key).map_err(|_| anyhow::anyhow!("Missing required environment variable: {}", key))
}

/// Load an environment variable with a default value.
///
/// Returns the parsed environment variable if set, otherwise returns the default.
///
/// # Errors
///
/// Returns an error if the variable is set but cannot be parsed.
fn env_or<T>(key: &str, default: T) -> anyhow::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(val) => val
            .parse::<T>()
            .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", key, e)),
        Err(_) => Ok(default),
    }
}
