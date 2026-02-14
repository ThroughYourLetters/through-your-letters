use axum::{
    Json,
    extract::State,
};
use serde::{Deserialize, Serialize};

use crate::presentation::http::{
    errors::AppError,
    handlers::cities::{bootstrap_capitals_from_restcountries, discover_and_cache_cities},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct DiscoverCitiesRequest {
    pub query: String,
    pub country_code: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct BootstrapCapitalsRequest {
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct CitySyncResponse {
    pub processed: usize,
    pub upserted: usize,
    pub failed: usize,
}

pub async fn discover_cities(
    State(state): State<AppState>,
    Json(body): Json<DiscoverCitiesRequest>,
) -> Result<Json<CitySyncResponse>, AppError> {
    let query = body.query.trim();
    if query.len() < 2 {
        return Err(AppError::BadRequest(
            "query must be at least 2 characters".to_string(),
        ));
    }

    let limit = body.limit.unwrap_or(50).clamp(1, 100);
    let result = discover_and_cache_cities(&state, query, body.country_code.as_deref(), limit).await;

    Ok(Json(CitySyncResponse {
        processed: result.processed,
        upserted: result.upserted,
        failed: result.failed,
    }))
}

pub async fn bootstrap_capitals(
    State(state): State<AppState>,
    Json(body): Json<BootstrapCapitalsRequest>,
) -> Result<Json<CitySyncResponse>, AppError> {
    let limit = body.limit.unwrap_or(200).clamp(1, 500);
    let result = bootstrap_capitals_from_restcountries(&state, limit).await?;

    Ok(Json(CitySyncResponse {
        processed: result.processed,
        upserted: result.upserted,
        failed: result.failed,
    }))
}
