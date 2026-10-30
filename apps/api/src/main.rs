use api::{
    config::Config,
    infrastructure::{
        cache::response_cache::ResponseCache,
        database::{migrations::run_migrations, pool::create_pool},
        email::email_service::EmailService,
        geocoding::nominatim::Geocoder,
        images::ImageProcessor,
        ml::image_analyzer::ImageAnalyzer,
        queue::postgres_jobs::PostgresJobs,
        repositories::sqlx_lettering_repository::SqlxLetteringRepository,
        repositories::sqlx_social_repository::SqlxSocialRepository,
        security::{
            passwords::PasswordService, rate_limiter::RateLimiter, virus_scanner::VirusScanner,
        },
        shutdown::{Shutdown, supervise},
        storage::cleanup::StorageCleanup,
        storage::r2_storage_service::R2StorageService,
        storage::upload_attempts::PostgresUploadAttempts,
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

fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Initialize logging with safe environment filter
    // Uses RUST_LOG if set, otherwise uses sensible defaults
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new("info,api=debug,tower_http=debug"))
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let config = Config::from_env()?;
    let runtime_timeout = Duration::from_millis(config.limits.shutdown.runtime_timeout_ms);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(run(config));
    runtime.shutdown_timeout(runtime_timeout);
    result
}

async fn run(config: Config) -> anyhow::Result<()> {
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
    let detector = Arc::new(ImageAnalyzer::new(
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
        upload_attempts: Arc::new(PostgresUploadAttempts::new(
            db.clone(),
            config.limits.storage.clone(),
        )),
        queue,
        virus_scanner,
        email_service,
        config: config.clone(),
        lettering_repo: Arc::new(SqlxLetteringRepository::new(db.clone())),
        social_repo: Arc::new(SqlxSocialRepository::new(db.clone())),
        ws_broadcaster: broadcaster.clone(),
    };

    let shutdown = Shutdown::new(
        Duration::from_secs(config.limits.shutdown.grace_seconds),
        Duration::from_secs(config.limits.shutdown.release_margin_seconds),
    );
    let signals = shutdown_signal()?;

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
            .map(|origin| {
                origin.parse::<HeaderValue>().map_err(|_| {
                    anyhow::anyhow!("ALLOWED_ORIGINS contains an invalid HTTP header value")
                })
            })
            .collect::<Result<_, _>>()?;

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

    let app = create_router(state.clone())
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
    let mut workers = tokio::task::JoinSet::<(&'static str, anyhow::Result<()>)>::new();
    if config.enable_ml_processing && config.enable_ml_worker {
        let worker = MlProcessor::new(
            db.clone(),
            detector,
            state.queue.clone(),
            config
                .huggingface_token
                .clone()
                .filter(|token| !token.trim().is_empty())
                .map(|token| {
                    config.limits.ocr.endpoint.parse().map(|endpoint| {
                        api::infrastructure::ml::ocr::OcrService::new(
                            endpoint,
                            token,
                            config.limits.images.max_provider_response_bytes,
                        )
                    })
                })
                .transpose()?,
            broadcaster,
            state.cache.clone(),
        );
        let stop = shutdown.clone();
        workers.spawn(async move { ("analysis", worker.start(stop).await) });
    } else {
        tracing::info!("Analysis worker disabled on this instance");
    }
    let analytics = AnalyticsWorker::new(db.clone());
    let stop = shutdown.clone();
    workers.spawn(async move {
        analytics.start(stop).await;
        ("analytics", Ok(()))
    });
    let cleanup = StorageCleanup::new(
        db.clone(),
        state.storage.clone(),
        config.limits.storage.clone(),
    );
    let stop = shutdown.clone();
    workers.spawn(async move {
        cleanup.start(stop).await;
        ("storage_cleanup", Ok(()))
    });
    let reconciliation = state.upload_attempts.clone();
    let stop = shutdown.clone();
    workers.spawn(async move {
        reconciliation.start(stop).await;
        ("upload_reconciliation", Ok(()))
    });
    if config.enable_pending_auto_approve {
        let worker = PendingAutoApproveWorker::new(
            db.clone(),
            state.ws_broadcaster.clone(),
            config.pending_auto_approve_minutes,
            config.pending_auto_approve_interval_seconds,
            config.pending_auto_approve_batch_size,
            state.cache.clone(),
        );
        let stop = shutdown.clone();
        workers.spawn(async move {
            worker.start(stop).await;
            ("pending_approval", Ok(()))
        });
    }
    let stop = shutdown.clone();
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        stop.wait().await;
    });
    let stop = shutdown.clone();
    let signal_task = tokio::spawn(async move {
        let result = signals.await;
        stop.request();
        result
    });
    let result = supervise(async move { server.await }, shutdown, workers).await;
    if signal_task.is_finished() {
        signal_task.await??;
    } else {
        signal_task.abort();
    }
    result
}

#[cfg(unix)]
fn shutdown_signal() -> std::io::Result<impl std::future::Future<Output = std::io::Result<()>>> {
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    Ok(async move {
        let signal = tokio::select! {
            signal = interrupt.recv() => signal,
            signal = terminate.recv() => signal,
        };
        signal.ok_or_else(|| std::io::Error::other("Shutdown signal listener closed"))?;
        tracing::info!("Shutdown requested");
        Ok(())
    })
}

#[cfg(not(unix))]
fn shutdown_signal() -> std::io::Result<impl std::future::Future<Output = std::io::Result<()>>> {
    Ok(async {
        tokio::signal::ctrl_c().await?;
        tracing::info!("Shutdown requested");
        Ok(())
    })
}
