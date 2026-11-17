use crate::{
    infrastructure::geocoding::nominatim::GeocodeError,
    presentation::http::{
        errors::AppError,
        middleware::{client_ip::ClientIp, rate_limit::enforce},
        pagination::Page,
        state::AppState,
    },
};
use axum::{
    Json,
    extract::{Extension, Query, State},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceSearchQuery {
    pub q: String,
    pub country_code: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Serialize, TS)]
pub struct PlaceSuggestion {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// Search only after the contributor submits a query; this endpoint is not autocomplete.
pub async fn search_places(
    State(state): State<AppState>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    Query(query): Query<PlaceSearchQuery>,
) -> Result<Json<Vec<PlaceSuggestion>>, AppError> {
    let q = query.q.trim();
    if !(2..=200).contains(&q.chars().count()) {
        return Err(AppError::BadRequest(
            "Enter a place search of 2 to 200 characters".into(),
        ));
    }
    let page = Page::from_limits(
        query.limit,
        0,
        &state.config.limits.pages.places,
        &state.config.limits.pagination,
    )?;
    enforce(
        &state,
        &format!("place-search:{ip}"),
        &state.config.limits.security.place_search,
    )
    .await?;
    let places = state
        .geocoder
        .cities(q, query.country_code.as_deref(), page.limit())
        .await
        .map_err(search_error)?;
    let mut suggestions = Vec::with_capacity(places.len());
    for place in places.into_iter().take(page.limit() as usize) {
        let point = place.coordinates().map_err(search_error)?;
        let name = if place.display_name.trim().is_empty() {
            place.name.as_deref().unwrap_or("Selected place")
        } else {
            &place.display_name
        };
        suggestions.push(PlaceSuggestion {
            name: name.trim().chars().take(200).collect(),
            latitude: point.coordinates[1],
            longitude: point.coordinates[0],
        });
    }
    Ok(Json(suggestions))
}

fn search_error(error: GeocodeError) -> AppError {
    match error {
        GeocodeError::InvalidQuery => {
            AppError::BadRequest("Invalid place search or country code".into())
        }
        GeocodeError::Database(error) => AppError::database(error),
        _ => AppError::ExternalService(anyhow::anyhow!(
            "Place search is unavailable; choose a point on the map or retry later"
        )),
    }
}
