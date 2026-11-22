use crate::presentation::http::{middleware::public_revision::PublicRevision, state::AppState};
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;

#[derive(Clone, Copy, Serialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
}

#[derive(Clone, Copy, Serialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum DependencyStatus {
    Up,
    Down,
}

#[derive(Serialize, ts_rs::TS)]
pub struct HealthResponse {
    pub status: HealthStatus,
    pub database: DependencyStatus,
    pub cache_consistency: DependencyStatus,
    pub version: &'static str,
}

/// Readiness requires the committed revision used to validate public cache hits.
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let started = std::time::Instant::now();
    let (status, database, cache_consistency) = match PublicRevision::load(&state).await {
        Ok(_) => (
            HealthStatus::Healthy,
            DependencyStatus::Up,
            DependencyStatus::Up,
        ),
        Err(error) => {
            let database_responded = matches!(
                error.downcast_ref::<sqlx::Error>(),
                Some(sqlx::Error::Database(_) | sqlx::Error::RowNotFound)
            );
            tracing::error!(
                component = "public_cache_revision",
                timeout = error.downcast_ref::<tokio::time::error::Elapsed>().is_some(),
                elapsed_ms = started.elapsed().as_millis(),
                sqlstate = ?error.downcast_ref::<sqlx::Error>()
                    .and_then(|error| error.as_database_error())
                    .and_then(|error| error.code()),
                "Readiness check failed"
            );
            (
                HealthStatus::Unhealthy,
                if database_responded {
                    DependencyStatus::Up
                } else {
                    DependencyStatus::Down
                },
                DependencyStatus::Down,
            )
        }
    };
    let code = match status {
        HealthStatus::Healthy => StatusCode::OK,
        HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };
    (
        code,
        Json(HealthResponse {
            status,
            database,
            cache_consistency,
            version: env!("CARGO_PKG_VERSION"),
        }),
    )
}
