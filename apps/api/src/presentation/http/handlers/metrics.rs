use super::super::state::AppState;
use crate::infrastructure::monitoring::HttpMetricsSnapshot;
use axum::{Json, extract::State};

/// Protected by the administrator router. Measurements reset with this instance.
pub async fn metrics(State(state): State<AppState>) -> Json<HttpMetricsSnapshot> {
    Json(state.metrics.snapshot())
}
