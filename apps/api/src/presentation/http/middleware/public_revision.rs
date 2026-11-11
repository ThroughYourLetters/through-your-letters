use crate::presentation::http::{errors::AppError, state::AppState};
use anyhow::Context;
use axum::{
    extract::{Request, State},
    http::Method,
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};
use std::time::Duration;
use uuid::Uuid;

/// A request's committed public-data version, obtained before reading cached data.
#[derive(Clone)]
pub struct PublicRevision {
    epoch: Uuid,
    revision: String,
}

impl PublicRevision {
    pub(crate) async fn load(state: &AppState) -> anyhow::Result<Self> {
        let (epoch, writers) = tokio::time::timeout(
            Duration::from_millis(state.config.limits.cache.freshness_timeout_ms),
            sqlx::query_as::<_, (Uuid, sqlx::types::Json<Vec<(i32, i64)>>)>(
                "SELECT epoch, COALESCE((SELECT json_agg(json_build_array(backend_pid,revision) ORDER BY backend_pid)
                 FROM public_cache_writers),'[]'::json)
                 FROM public_cache_revision WHERE singleton AND NOT pg_is_in_recovery()",
            )
            .fetch_one(&state.db),
        )
        .await
        .context("Public cache revision timed out")?
        .context("Public cache revision unavailable")?;
        let mut digest = Sha256::new();
        for (backend, revision) in writers.0 {
            digest.update(backend.to_be_bytes());
            digest.update(revision.to_be_bytes());
        }
        Ok(Self {
            epoch,
            revision: format!("{:x}", digest.finalize()),
        })
    }

    pub fn key(&self, key: &str) -> String {
        format!("public:{}:{}:{key}", self.epoch, self.revision)
    }
}

pub async fn require_public_revision(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    if matches!(*request.method(), Method::GET | Method::HEAD) {
        let revision = PublicRevision::load(&state)
            .await
            .map_err(AppError::ExternalService)?;
        request.extensions_mut().insert(revision);
    }
    Ok(next.run(request).await)
}
