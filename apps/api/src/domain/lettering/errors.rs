use thiserror::Error;

#[derive(Error)]
pub enum DomainError {
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Not found")]
    NotFound(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Infrastructure error")]
    InfrastructureError(#[source] anyhow::Error),
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    #[error("Unauthorized")]
    Unauthorized,
}

impl std::fmt::Debug for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::Conflict(_) => "conflict",
            Self::NotFound(_) => "not_found",
            Self::ValidationError(_) => "validation",
            Self::InfrastructureError(_) => "infrastructure",
            Self::RateLimitExceeded => "rate_limited",
            Self::Unauthorized => "unauthorized",
        };
        f.debug_struct("DomainError").field("kind", &kind).finish()
    }
}

impl DomainError {
    pub fn infrastructure(error: impl Into<anyhow::Error>) -> Self {
        Self::InfrastructureError(error.into())
    }
}
