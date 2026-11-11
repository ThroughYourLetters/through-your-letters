use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;

use crate::{
    infrastructure::repositories::sqlx_lettering_repository::SqlxLetteringRepository,
    presentation::http::contracts::PublicLettering,
    presentation::http::{errors::AppError, state::AppState},
};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    q: String,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    lang: Option<String>,
}

fn default_limit() -> i64 {
    20
}

pub async fn search_letterings(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<PublicLettering>>, AppError> {
    let page = crate::presentation::http::pagination::Page::new(
        params.limit,
        params.offset,
        100,
        &state.config.limits.pagination,
    )?;

    let q = params.q.trim();
    if q.is_empty() {
        return Ok(Json(vec![]));
    }

    let repository = SqlxLetteringRepository::new(state.db.clone());
    let results = repository
        .search_with_locale(q, params.lang.as_deref(), page.limit(), page.offset())
        .await
        .map_err(AppError::internal)?;

    Ok(Json(results.into_iter().map(Into::into).collect()))
}
