use axum::{Json, extract::State};
use serde::Serialize;

use crate::presentation::http::{errors::AppError, state::AppState};

#[derive(Debug, Serialize)]
pub struct NeighborhoodCount {
    pub pin_code: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct NeighborhoodsResponse {
    pub neighborhoods: Vec<NeighborhoodCount>,
}

pub async fn get_neighborhoods(
    State(state): State<AppState>,
) -> Result<Json<NeighborhoodsResponse>, AppError> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT pin_code, COUNT(*)::bigint AS count \
         FROM letterings \
         WHERE status = 'APPROVED' \
         GROUP BY pin_code \
         ORDER BY count DESC \
         LIMIT 200",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    let neighborhoods = rows
        .into_iter()
        .map(|(pin_code, count)| NeighborhoodCount { pin_code, count })
        .collect();

    Ok(Json(NeighborhoodsResponse { neighborhoods }))
}
