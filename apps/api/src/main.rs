use api::{
    config::Config,
    infrastructure::{
        cache::response_cache::ResponseCache,
        database::{migrations::run_migrations, pool::create_pool},
        email::email_service::EmailService,
        geocoding::nominatim::Geocoder,
        images::ImageProcessor,
        ml::onnx_text_detector::OnnxTextDetector,
        queue::postgres_jobs::PostgresJobs,
        repositories::sqlx_lettering_repository::SqlxLetteringRepository,
        repositories::sqlx_social_repository::SqlxSocialRepository,
        security::{
            passwords::PasswordService, rate_limiter::RateLimiter, virus_scanner::VirusScanner,
        },
        storage::cleanup::StorageCleanup,
        storage::r2_storage_service::R2StorageService,
    },
    presentation::http::{routes::create_router, state::AppState},
    workers::{
        analytics_worker::AnalyticsWorker, ml_processor::MlProcessor,
        pending_auto_approve::PendingAutoApproveWorker,
    },
};
use http::{HeaderValue, Method, header};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Initialize logging with safe environment filter
    // Uses RUST_LOG if set, otherwise uses sensible defaults
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new("info,api=debug,tower_http=debug"))
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let config = Config::from_env()?;
    tracing::info!(
        monthly_commands = config.limits.redis_budget.monthly_commands,
        reserved_commands = config.limits.redis_budget.reserved_commands,
        "Redis command budget loaded"
    );
    let db = create_pool(&config.database_url, config.database_max_connections).await?;
    run_migrations(&db, config.ignore_missing_migrations).await?;

    let redis = redis::Client::open(config.redis_url.clone())?;
    tracing::info!(
        render_ingress = config.render_ingress,
        trusted_proxy_networks = config.trusted_proxy_cidrs.len(),
        "Client address trust configured"
    );
    let cache = Arc::new(ResponseCache::new(config.limits.cache.clone()));
    let queue = Arc::new(PostgresJobs::new(db.clone(), config.limits.queue.clone()));
    let storage = Arc::new(
        R2StorageService::new(
            config.r2_access_key_id.clone(),
            config.r2_secret_access_key.clone(),
            config.r2_endpoint.clone(),
            config.r2_region.clone(),
            config.r2_force_path_style,
            config.r2_bucket_name.clone(),
            config.r2_public_url.clone(),
        )
        .await?,
    );

    let virus_scanner = Arc::new(VirusScanner::new(
        config.enable_virus_scan,
        std::env::var("CLAMAV_HOST").ok(),
        std::env::var("CLAMAV_PORT")
            .ok()
            .and_then(|p| p.parse().ok()),
        Duration::from_millis(config.limits.images.scan_timeout_ms),
    ));

    let (tx, _) = broadcast::channel(100);
    let broadcaster = Arc::new(tx);
    let images = Arc::new(ImageProcessor::new(config.limits.images.clone()));
    let detector = Arc::new(OnnxTextDetector::new(
        &config.ml_model_path,
        config.enable_ml_processing,
        images.clone(),
    )?);

    let email_service = Arc::new(EmailService::new(
        config.resend_api_key.clone(),
        config.email_from.clone(),
        config.app_base_url.clone(),
        &config.limits.email,
    )?);
    if !email_service.is_configured() {
        tracing::warn!("Email delivery disabled: RESEND_API_KEY is not configured");
    }

    let state = AppState {
        db: db.clone(),
        rate_limiter: Arc::new(RateLimiter::new(redis, &config.limits.security)),
        passwords: Arc::new(PasswordService::new(&config.limits.passwords)),
        images,
        cache,
        metrics: Arc::new(api::infrastructure::monitoring::HttpMetrics::default()),
        geocoder: Geocoder::new(
            db.clone(),
            config.limits.geocoding.clone(),
            config
                .city_discovery_user_agent
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("through-your-letters/1.0 (contact@throughyourletters.online)"),
        )?,
        storage,
        ml_detector: detector.clone(),
        queue,
        virus_scanner,
        email_service,
        config: config.clone(),
        lettering_repo: Arc::new(SqlxLetteringRepository::new(db.clone())),
        social_repo: Arc::new(SqlxSocialRepository::new(db.clone())),
        ws_broadcaster: broadcaster.clone(),
    };

    if config.enable_ml_processing && config.enable_ml_worker {
        let ml_worker = MlProcessor::new(
            db.clone(),
            detector,
            state.queue.clone(),
            config.huggingface_token.clone(),
            broadcaster,
            state.cache.clone(),
        );
        tokio::spawn(async move {
            if ml_worker.start().await.is_err() {
                tracing::error!("Analysis worker stopped");
            }
        });
    } else {
        tracing::info!("Analysis worker disabled on this instance");
    }

    let analytics = AnalyticsWorker::new(db.clone());
    tokio::spawn(async move { analytics.start().await });
    let cleanup = StorageCleanup::new(
        db.clone(),
        state.storage.clone(),
        config.limits.storage.clone(),
    );
    tokio::spawn(async move { cleanup.start().await });

    if config.enable_pending_auto_approve {
        let pending_worker = PendingAutoApproveWorker::new(
            db.clone(),
            state.ws_broadcaster.clone(),
            config.pending_auto_approve_minutes,
            config.pending_auto_approve_interval_seconds,
            config.pending_auto_approve_batch_size,
            state.cache.clone(),
        );
        tokio::spawn(async move { pending_worker.start().await });
    }

    // Configure CORS
    let cors = if cfg!(debug_assertions) {
        // Development: allow any origin
        CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                header::CONTENT_TYPE,
                header::AUTHORIZATION,
                header::ACCEPT,
                header::HeaderName::from_static("idempotency-key"),
            ])
            .max_age(Duration::from_secs(3600))
    } else {
        // Production: use explicitly configured origins.
        // Fail at startup if none are configured — a silent CORS rejection
        // at runtime is much harder to debug than a loud startup failure.
        if config.allowed_origins.is_empty() {
            anyhow::bail!(
                "ALLOWED_ORIGINS is not set. In production, you must set ALLOWED_ORIGINS \
                 to a comma-separated list of allowed origins, e.g.: \
                 ALLOWED_ORIGINS=https://www.throughyourletters.online,https://throughyourletters.online"
            );
        }

        let origins: Vec<HeaderValue> = config
            .allowed_origins
            .iter()
            .map(|o| {
                o.parse::<HeaderValue>().unwrap_or_else(|_| {
                    panic!("Invalid origin in ALLOWED_ORIGINS: '{}'. Must be a valid HTTP header value.", o)
                })
            })
            .collect();

        tracing::info!(
            origin_count = config.allowed_origins.len(),
            "CORS allowlist configured"
        );

        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                header::CONTENT_TYPE,
                header::AUTHORIZATION,
                header::ACCEPT,
                header::HeaderName::from_static("idempotency-key"),
            ])
            .max_age(Duration::from_secs(3600))
    };

    let app = create_router(state)
        .layer(cors)
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ));

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("ARCHIVE ONLINE AT {}", addr);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Ctrl+C received, initiating graceful shutdown");
        }
        _ = terminate => {
            tracing::info!("SIGTERM received, initiating graceful shutdown");
        }
    }
}

// Admin password hashing utility has been moved to:
// scripts/generate_admin_hash.rs
// Run with: cargo run --manifest-path scripts/Cargo.toml --bin generate_admin_hash
