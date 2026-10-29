use crate::{
    config::Config,
    infrastructure::{
        cache::response_cache::ResponseCache,
        email::email_service::EmailService,
        geocoding::nominatim::Geocoder,
        ml::traits::MlService,
        queue::redis_queue::RedisQueue,
        repositories::{
            sqlx_lettering_repository::SqlxLetteringRepository,
            sqlx_social_repository::SqlxSocialRepository,
        },
        security::{rate_limiter::RateLimiter, virus_scanner::VirusScanner},
        storage::traits::StorageService,
    },
};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub rate_limiter: Arc<RateLimiter>,
    pub cache: Arc<ResponseCache>,
    pub geocoder: Geocoder,
    pub storage: Arc<dyn StorageService>,
    pub ml_detector: Arc<dyn MlService>,
    pub queue: Arc<RedisQueue>,
    pub virus_scanner: Arc<VirusScanner>,
    pub email_service: Arc<EmailService>,
    pub config: Config,
    pub lettering_repo: Arc<SqlxLetteringRepository>,
    pub social_repo: Arc<SqlxSocialRepository>,
    pub ws_broadcaster: Arc<broadcast::Sender<String>>,
}
