use crate::{
    config::Config,
    infrastructure::{
        cache::redis_cache::RedisCache,
        ml::traits::MlService,
        queue::redis_queue::RedisQueue,
        repositories::{
            sqlx_lettering_repository::SqlxLetteringRepository,
            sqlx_social_repository::SqlxSocialRepository,
        },
        security::virus_scanner::VirusScanner,
        storage::traits::StorageService,
    },
};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: redis::Client,
    pub cache: Arc<RedisCache>,
    pub storage: Arc<dyn StorageService>,
    pub ml_detector: Arc<dyn MlService>,
    pub queue: Arc<RedisQueue>,
    pub virus_scanner: Arc<VirusScanner>,
    pub config: Config,
    pub lettering_repo: Arc<SqlxLetteringRepository>,
    pub social_repo: Arc<SqlxSocialRepository>,
    pub ws_broadcaster: Arc<broadcast::Sender<String>>,
}
