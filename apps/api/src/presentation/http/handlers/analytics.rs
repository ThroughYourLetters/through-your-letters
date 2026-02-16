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
    let rows = sqlx::query!(
        r#"SELECT pin_code, COUNT(*) as "artifact_count!" FROM letterings WHERE status = 'APPROVED' GROUP BY pin_code ORDER BY "artifact_count!" DESC"#
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    let neighborhoods = rows
        .into_iter()
        .map(|r| NeighborhoodCount {
            pin_code: r.pin_code,
            count: r.artifact_count,
        })
        .collect();

    Ok(Json(NeighborhoodsResponse { neighborhoods }))
}
