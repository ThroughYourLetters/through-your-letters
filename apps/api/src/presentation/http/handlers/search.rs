use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;

use crate::{
    domain::lettering::entity::Lettering,
    infrastructure::repositories::sqlx_lettering_repository::SqlxLetteringRepository,
    presentation::http::{errors::AppError, state::AppState},
};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    q: String,
    #[serde(default = "default_limit")]
    limit: i64,
    lang: Option<String>,
}

fn default_limit() -> i64 {
    20
}

pub async fn search_letterings(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<Lettering>>, AppError> {
    let q = params.q.trim();
    if q.is_empty() {
        return Ok(Json(vec![]));
    }

    let repository = SqlxLetteringRepository::new(state.db.clone());
    let results = repository
        .search_with_locale(q, params.lang.as_deref(), params.limit.clamp(1, 100))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(results))
}
