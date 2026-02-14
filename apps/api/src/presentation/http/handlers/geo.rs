use crate::presentation::http::{errors::AppError, state::AppState};
use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Serialize)]
pub struct Marker {
    pub id: uuid::Uuid,
    pub lat: f64,
    pub lng: f64,
    pub thumbnail: String,
}

#[derive(Serialize)]
pub struct CoveragePoint {
    pub pin_code: String,
    pub city_id: uuid::Uuid,
    pub city_name: String,
    pub lat: f64,
    pub lng: f64,
    pub count: i64,
}

#[derive(Deserialize)]
pub struct NearbyQuery {
    pub lat: f64,
    pub lng: f64,
    pub radius_m: f64,
}

#[derive(Deserialize, Default)]
pub struct MarkersQuery {
    pub city_id: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Deserialize, Default)]
pub struct CoverageQuery {
    pub city_id: Option<Uuid>,
    pub min_count: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn get_all_markers(
    State(state): State<AppState>,
    Query(params): Query<MarkersQuery>,
) -> Result<Json<Vec<Marker>>, AppError> {
    let mut qb = QueryBuilder::<Postgres>::new(
        "SELECT l.id, COALESCE(l.thumbnail_small, '') as thumbnail_small, ST_Y(l.location::geometry) as lat, ST_X(l.location::geometry) as lng
         FROM letterings l
         JOIN cities c ON c.id = l.city_id
         LEFT JOIN region_policies rp ON rp.country_code = c.country_code
         WHERE l.status = 'APPROVED'
           AND COALESCE(rp.discoverability_enabled, true)",
    );

    if let Some(city_id) = params.city_id {
        qb.push(" AND l.city_id = ");
        qb.push_bind(city_id);
    }

    qb.push(" ORDER BY l.created_at DESC LIMIT ");
    qb.push_bind(params.limit.unwrap_or(3000).clamp(1, 10000));

    let rows: Vec<(Uuid, String, f64, f64)> = qb
        .build_query_as()
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, thumbnail, lat, lng)| Marker {
                id,
                lat,
                lng,
                thumbnail,
            })
            .collect(),
    ))
}

pub async fn get_nearby_markers(
    State(state): State<AppState>,
    Query(q): Query<NearbyQuery>,
) -> Result<Json<Vec<Marker>>, AppError> {
    let rows: Vec<(Uuid, String, f64, f64)> = sqlx::query_as(
        r#"SELECT l.id, COALESCE(l.thumbnail_small, '') as thumbnail_small, ST_Y(l.location::geometry) as lat, ST_X(l.location::geometry) as lng
           FROM letterings l
           JOIN cities c ON c.id = l.city_id
           LEFT JOIN region_policies rp ON rp.country_code = c.country_code
           WHERE l.status = 'APPROVED'
             AND COALESCE(rp.discoverability_enabled, true)
             AND ST_DWithin(l.location, ST_SetSRID(ST_MakePoint($1, $2), 4326)::geography, $3)"#,
    )
    .bind(q.lng)
    .bind(q.lat)
    .bind(q.radius_m)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::InternalError(e.to_string()))?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, thumbnail, lat, lng)| Marker {
                id,
                lat,
                lng,
                thumbnail,
            })
            .collect(),
    ))
}

pub async fn get_coverage(
    State(state): State<AppState>,
    Query(params): Query<CoverageQuery>,
) -> Result<Json<Vec<CoveragePoint>>, AppError> {
    let mut qb = QueryBuilder::<Postgres>::new(
        "SELECT l.pin_code, l.city_id, c.name, AVG(ST_Y(l.location::geometry))::double precision as lat, AVG(ST_X(l.location::geometry))::double precision as lng, COUNT(*)::bigint as count
         FROM letterings l
         JOIN cities c ON c.id = l.city_id
         LEFT JOIN region_policies rp ON rp.country_code = c.country_code
         WHERE l.status = 'APPROVED'
           AND COALESCE(rp.discoverability_enabled, true)",
    );

    if let Some(city_id) = params.city_id {
        qb.push(" AND l.city_id = ");
        qb.push_bind(city_id);
    }

    qb.push(" GROUP BY l.pin_code, l.city_id, c.name");

    if let Some(min_count) = params.min_count {
        qb.push(" HAVING COUNT(*) >= ");
        qb.push_bind(min_count.max(1));
    }

    qb.push(" ORDER BY COUNT(*) DESC LIMIT ");
    qb.push_bind(params.limit.unwrap_or(5000).clamp(1, 20000));

    let rows: Vec<(String, uuid::Uuid, String, f64, f64, i64)> = qb
        .build_query_as()
        .fetch_all(&state.db)
        .await
        .map_err(|e: sqlx::Error| AppError::InternalError(e.to_string()))?;

    Ok(Json(
        rows.into_iter()
            .map(|(pin_code, city_id, city_name, lat, lng, count)| CoveragePoint {
                pin_code,
                city_id,
                city_name,
                lat,
                lng,
                count,
            })
            .collect(),
    ))
}
