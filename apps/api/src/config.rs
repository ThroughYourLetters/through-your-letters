//! Application configuration loading from environment variables.
//!
//! Resource budgets come from `runtime.json` or `TTL_LIMITS_FILE`. Environment
//! overrides are applied before validation; credentials remain in the environment.
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
//! - `RENDER`, `RENDER_SERVICE_TYPE`: Platform-provided values enable Render web ingress automatically
//! - `TRUSTED_PROXY_CIDRS`: Verified proxy networks for non-Render deployments (default: none)
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
//! - `IGNORE_MISSING_MIGRATIONS`: Explicit recovery override for missing history (default: false)
//! - `ALLOWED_ORIGINS`: Comma-separated list of allowed CORS origins (required in production)
//! - `RESEND_API_KEY`: Resend.com API key for transactional email (optional; disabled delivery is reported)
//! - `EMAIL_FROM`: From address for outgoing emails (default: "noreply@throughyourletters.online")
//! - `APP_BASE_URL`: Public base URL used in email links (default: "https://www.throughyourletters.online")

use serde::Deserialize;

/// Complete server configuration loaded from environment.
///
/// Represents the full configuration state of the application. All fields are populated from
/// environment variables at startup, with sensible defaults provided where appropriate.
#[derive(Clone, Deserialize)]
pub struct Config {
    pub limits: crate::limits::Limits,
    /// PostgreSQL connection string (e.g., `postgres://user:pass@localhost/db`)
    pub database_url: String,

    /// Redis connection URL for shared abuse limits
    pub redis_url: String,

    /// Use Cloudflare's client address on a Render web service's managed ingress.
    pub render_ingress: bool,

    /// Immediate proxy peers and intermediate hops allowed to supply client addresses.
    pub trusted_proxy_cidrs: Vec<sqlx::types::ipnetwork::IpNetwork>,

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

    /// Secret key for JWT token signing and verification (user tokens)
    pub jwt_secret: String,

    /// Secret key for admin JWT token signing and verification.
    /// Defaults to jwt_secret if ADMIN_JWT_SECRET is not set, but production deployments
    /// MUST set this separately to prevent privilege escalation via shared secrets.
    pub admin_jwt_secret: String,

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

    /// Enable analysis processing on this instance; claims are safe across workers.
    pub enable_ml_worker: bool,

    /// Path to ONNX model file for text detection
    pub ml_model_path: String,

    /// Enable virus scanning via ClamAV
    pub enable_virus_scan: bool,
    pub clamav_host: Option<String>,
    pub clamav_port: Option<u16>,

    /// Rate limit: maximum uploads per IP address per day
    pub rate_limit_uploads_per_ip: u32,

    /// Enable automatic approval of pending letterings
    pub enable_pending_auto_approve: bool,

    /// Recovery override for applied versions absent from this binary; false by default.
    pub ignore_missing_migrations: bool,

    /// Allowed CORS origins (e.g., ["https://www.throughyourletters.online", "https://throughyourletters.online"])
    /// Loaded from ALLOWED_ORIGINS env var as comma-separated values.
    /// In production, if this is empty, CORS will reject all cross-origin requests.
    pub allowed_origins: Vec<String>,

    /// Resend API key for transactional email delivery.
    /// When absent or blank, delivery returns a safe error and startup emits a warning.
    pub resend_api_key: Option<String>,

    /// From address used in outgoing emails (e.g., "noreply@throughyourletters.online").
    pub email_from: String,

    /// Public base URL of the application, used to build links in emails
    /// (e.g., "https://www.throughyourletters.online").
    pub app_base_url: String,
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
        let limits = crate::limits::Limits::load_with_overrides(|limits| {
            limits.database.max_connections =
                env_or("DATABASE_MAX_CONNECTIONS", limits.database.max_connections)?;
            limits.workers.approval_delay_minutes = env_or(
                "PENDING_AUTO_APPROVE_MINUTES",
                limits.workers.approval_delay_minutes,
            )?;
            limits.workers.approval_poll_seconds = env_or(
                "PENDING_AUTO_APPROVE_INTERVAL_SECONDS",
                limits.workers.approval_poll_seconds,
            )?;
            limits.workers.approval_batch_size = env_or(
                "PENDING_AUTO_APPROVE_BATCH_SIZE",
                limits.workers.approval_batch_size,
            )?;
            Ok(())
        })?;
        let jwt_secret = env_required("JWT_SECRET")?;
        let admin_jwt_secret =
            env_optional("ADMIN_JWT_SECRET")?.unwrap_or_else(|| jwt_secret.clone());
        anyhow::ensure!(
            !admin_jwt_secret.trim().is_empty(),
            "ADMIN_JWT_SECRET must not be blank"
        );
        let clamav_port = env_optional("CLAMAV_PORT")?
            .map(|port| {
                port.parse::<u16>()
                    .map_err(|_| anyhow::anyhow!("CLAMAV_PORT must be a port number"))
            })
            .transpose()?;
        anyhow::ensure!(clamav_port != Some(0), "CLAMAV_PORT must be nonzero");
        let rate_limit_uploads_per_ip =
            env_or("RATE_LIMIT_UPLOADS_PER_IP", limits.uploads_per_ip_per_day)?;
        anyhow::ensure!(
            rate_limit_uploads_per_ip > 0,
            "RATE_LIMIT_UPLOADS_PER_IP must be positive"
        );
        let trusted_proxy_cidrs =
            parse_trusted_proxies(&env_optional("TRUSTED_PROXY_CIDRS")?.unwrap_or_default())?;
        Ok(Self {
            rate_limit_uploads_per_ip,
            limits,
            database_url: env_required("DATABASE_URL")?,
            redis_url: env_required("REDIS_URL")?,
            render_ingress: env_optional("RENDER")?.as_deref() == Some("true")
                && env_optional("RENDER_SERVICE_TYPE")?.as_deref() == Some("web"),
            trusted_proxy_cidrs,
            r2_access_key_id: env_required("R2_ACCESS_KEY_ID")?,
            r2_secret_access_key: env_required("R2_SECRET_ACCESS_KEY")?,
            r2_endpoint: env_required("R2_ENDPOINT")?,
            r2_region: env_or("R2_REGION", "auto".to_string())?,
            r2_force_path_style: env_or("R2_FORCE_PATH_STYLE", false)?,
            r2_bucket_name: env_required("R2_BUCKET_NAME")?,
            r2_public_url: env_required("R2_PUBLIC_URL")?,
            host: env_or("HOST", "0.0.0.0".to_string())?,
            port: env_or("PORT", 3000)?,
            jwt_secret,
            admin_jwt_secret,
            admin_email: env_required("ADMIN_EMAIL")?,
            admin_password_hash: env_required("ADMIN_PASSWORD_HASH")?,
            city_discovery_user_agent: env_optional("CITY_DISCOVERY_USER_AGENT")?,
            huggingface_token: env_optional("HUGGINGFACE_TOKEN")?,
            enable_ml_processing: env_or("ENABLE_ML_PROCESSING", true)?,
            enable_ml_worker: env_or("ENABLE_ML_WORKER", true)?,
            ml_model_path: env_or("ML_MODEL_PATH", "./models/text_detector.onnx".to_string())?,
            enable_virus_scan: env_or("ENABLE_VIRUS_SCAN", false)?,
            clamav_host: env_optional("CLAMAV_HOST")?,
            clamav_port,
            enable_pending_auto_approve: env_or("ENABLE_PENDING_AUTO_APPROVE", true)?,
            ignore_missing_migrations: env_or("IGNORE_MISSING_MIGRATIONS", false)?,
            allowed_origins: env_optional("ALLOWED_ORIGINS")?
                .map(|s| {
                    s.split(',')
                        .map(|o| o.trim().to_string())
                        .filter(|o| !o.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            resend_api_key: env_optional("RESEND_API_KEY")?,
            email_from: env_or(
                "EMAIL_FROM",
                "noreply@throughyourletters.online".to_string(),
            )?,
            app_base_url: env_or(
                "APP_BASE_URL",
                "https://www.throughyourletters.online".to_string(),
            )?,
        })
    }
}

fn parse_trusted_proxies(value: &str) -> anyhow::Result<Vec<sqlx::types::ipnetwork::IpNetwork>> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|part| {
            let network = part
                .trim()
                .parse::<sqlx::types::ipnetwork::IpNetwork>()
                .map_err(|_| {
                    anyhow::anyhow!("TRUSTED_PROXY_CIDRS must contain comma-separated IP networks")
                })?;
            anyhow::ensure!(
                network.prefix() > 0,
                "TRUSTED_PROXY_CIDRS must not trust every address"
            );
            Ok(network)
        })
        .collect()
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("limits", &self.limits)
            .field("host", &self.host)
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

fn optional_value(
    key: &str,
    value: Result<String, std::env::VarError>,
) -> anyhow::Result<Option<String>> {
    match value {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => anyhow::bail!("{key} must contain Unicode text"),
    }
}

fn env_optional(key: &str) -> anyhow::Result<Option<String>> {
    optional_value(key, std::env::var(key))
}

fn env_required(key: &str) -> anyhow::Result<String> {
    let value = env_optional(key)?
        .ok_or_else(|| anyhow::anyhow!("Missing required environment variable: {key}"))?;
    anyhow::ensure!(!value.trim().is_empty(), "{key} must not be blank");
    Ok(value)
}

fn value_or<T>(
    key: &str,
    value: Result<String, std::env::VarError>,
    default: T,
) -> anyhow::Result<T>
where
    T: std::str::FromStr,
{
    match optional_value(key, value)? {
        Some(value) => value
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid environment value for {key}")),
        None => Ok(default),
    }
}

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> anyhow::Result<T> {
    value_or(key, std::env::var(key), default)
}

#[cfg(test)]
mod proxy_tests {
    use super::{parse_trusted_proxies, value_or};

    #[test]
    fn environment_defaults_apply_only_to_missing_values() {
        assert_eq!(
            value_or("PORT", Err(std::env::VarError::NotPresent), 3000_u16).unwrap(),
            3000
        );
        assert_eq!(value_or("PORT", Ok("4000".into()), 3000_u16).unwrap(), 4000);
        assert!(value_or("PORT", Ok("invalid".into()), 3000_u16).is_err());
        let error = value_or(
            "PORT",
            Err(std::env::VarError::NotUnicode("private-value".into())),
            3000_u16,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "PORT must contain Unicode text");
    }

    #[test]
    fn proxy_trust_is_explicit_and_rejects_invalid_or_universal_networks() {
        assert!(parse_trusted_proxies("").unwrap().is_empty());
        assert_eq!(
            parse_trusted_proxies("10.0.0.5/32, 2001:db8::5/128")
                .unwrap()
                .len(),
            2
        );
        for value in [
            "*",
            "0.0.0.0/0",
            "::/0",
            "10.0.0.1/999",
            "10.0.0.1/32,",
            "proxy.example.com",
        ] {
            assert!(parse_trusted_proxies(value).is_err());
        }
    }
}
