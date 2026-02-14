use crate::presentation::http::state::AppState;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    database: &'static str,
    version: &'static str,
}

pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    // Check Database Connectivity
    let db_status = match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => "up",
        Err(e) => {
            tracing::error!("Health check failed: Database unreachable: {}", e);
            "down"
        }
    };

    let status = if db_status == "up" {
        "healthy"
    } else {
        "unhealthy"
    };

    let response = HealthResponse {
        status,
        database: db_status,
        version: env!("CARGO_PKG_VERSION"),
    };

    let code = if status == "healthy" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (code, Json(response))
}
