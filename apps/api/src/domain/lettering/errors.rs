use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Error, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum DomainError {
    #[error("Not found")]
    NotFound(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Infrastructure error: {0}")]
    InfrastructureError(String),
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    #[error("Unauthorized")]
    Unauthorized,
}
