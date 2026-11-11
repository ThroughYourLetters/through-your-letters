use crate::presentation::http::{errors::AppError, state::AppState};
use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
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
    pub location_source: crate::domain::lettering::entity::LocationSource,
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
    pub limit: Option<i64>,
    pub after: Option<String>,
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
    let page = crate::presentation::http::pagination::Page::new(
        params.limit.unwrap_or(3000),
        0,
        10000,
        &state.config.limits.pagination,
    )?;
    let mut qb = QueryBuilder::<Postgres>::new(
        "SELECT l.id, l.location_source, COALESCE(l.thumbnail_small, '') as thumbnail_small, ST_Y(l.location::geometry) as lat, ST_X(l.location::geometry) as lng
         FROM discoverable_letterings l
         WHERE l.location IS NOT NULL",
    );

    if let Some(city_id) = params.city_id {
        qb.push(" AND l.city_id = ");
        qb.push_bind(city_id);
    }

    qb.push(" ORDER BY l.created_at DESC, l.id DESC LIMIT ");
    qb.push_bind(page.limit());

    let rows: Vec<(
        Uuid,
        crate::domain::lettering::entity::LocationSource,
        String,
        f64,
        f64,
    )> = qb
        .build_query_as()
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, location_source, thumbnail, lat, lng)| Marker {
                id,
                lat,
                lng,
                thumbnail,
                location_source,
            })
            .collect(),
    ))
}

#[derive(sqlx::FromRow)]
struct NearbyRow {
    id: Uuid,
    location_source: crate::domain::lettering::entity::LocationSource,
    thumbnail: String,
    lat: f64,
    lng: f64,
    created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn get_nearby_markers(
    State(state): State<AppState>,
    Query(query): Query<NearbyQuery>,
) -> Result<(HeaderMap, Json<Vec<Marker>>), AppError> {
    use crate::presentation::http::pagination::{
        Page, TimePosition, continuation, decode_cursor, next_link,
    };
    use sha2::{Digest, Sha256};
    if !query.lat.is_finite()
        || !query.lng.is_finite()
        || !query.radius_m.is_finite()
        || !(-90.0..=90.0).contains(&query.lat)
        || !(-180.0..=180.0).contains(&query.lng)
        || !(0.0..=f64::from(state.config.limits.map.max_nearby_radius_m)).contains(&query.radius_m)
    {
        return Err(AppError::BadRequest(
            "Nearby coordinates or radius are outside the allowed range".into(),
        ));
    }
    let page = Page::new(
        query
            .limit
            .unwrap_or(state.config.limits.map.nearby_page_size.into()),
        0,
        state.config.limits.map.nearby_max_limit.into(),
        &state.config.limits.pagination,
    )?;
    let selection = serde_json::json!([query.lat, query.lng, query.radius_m]).to_string();
    let scope = format!("nearby:{:x}", Sha256::digest(selection.as_bytes()));
    let after: Option<TimePosition> = decode_cursor(
        query.after.as_deref(),
        &scope,
        &state.config.limits.pagination,
    )?;
    let mut sql = QueryBuilder::<Postgres>::new(
        "SELECT l.id, l.location_source, COALESCE(l.thumbnail_small, '') AS thumbnail,
         ST_Y(l.location::geometry) AS lat, ST_X(l.location::geometry) AS lng, l.created_at
         FROM discoverable_letterings l WHERE l.location IS NOT NULL AND ST_DWithin(l.location, ST_SetSRID(ST_MakePoint(",
    );
    sql.push_bind(query.lng)
        .push(",")
        .push_bind(query.lat)
        .push("),4326)::geography,")
        .push_bind(query.radius_m)
        .push(")");
    if let Some(after) = after {
        sql.push(" AND (l.created_at,l.id) < (")
            .push_bind(after.created_at)
            .push(",")
            .push_bind(after.id)
            .push(")");
    }
    sql.push(" ORDER BY l.created_at DESC,l.id DESC LIMIT ")
        .push_bind(page.limit() + 1);
    let mut rows: Vec<NearbyRow> = sql.build_query_as().fetch_all(&state.db).await?;
    let next = continuation(&mut rows, page, &scope, |row| TimePosition {
        created_at: row.created_at,
        id: row.id,
    })?;
    let headers = next_link(
        "/api/v1/geo/nearby",
        &[
            ("lat", query.lat.to_string()),
            ("lng", query.lng.to_string()),
            ("radius_m", query.radius_m.to_string()),
            ("limit", page.limit().to_string()),
        ],
        next.as_deref(),
    )?;
    let markers = rows
        .into_iter()
        .map(|row| Marker {
            id: row.id,
            location_source: row.location_source,
            thumbnail: row.thumbnail,
            lat: row.lat,
            lng: row.lng,
        })
        .collect();
    Ok((headers, Json(markers)))
}

pub async fn get_coverage(
    State(state): State<AppState>,
    Query(params): Query<CoverageQuery>,
) -> Result<Json<Vec<CoveragePoint>>, AppError> {
    let page = crate::presentation::http::pagination::Page::new(
        params.limit.unwrap_or(5000),
        0,
        20000,
        &state.config.limits.pagination,
    )?;
    let mut qb = QueryBuilder::<Postgres>::new(
        "SELECT g.pin_code, g.city_id, c.name, g.lat, g.lng, g.count
         FROM (SELECT l.pin_code, l.city_id,
         AVG(ST_Y(l.location::geometry))::double precision AS lat,
         AVG(ST_X(l.location::geometry))::double precision AS lng, COUNT(*)::bigint AS count
         FROM discoverable_letterings l
         WHERE l.location IS NOT NULL",
    );

    if let Some(city_id) = params.city_id {
        qb.push(" AND l.city_id = ");
        qb.push_bind(city_id);
    }

    qb.push(" GROUP BY l.pin_code, l.city_id");

    if let Some(min_count) = params.min_count {
        qb.push(" HAVING COUNT(*) >= ");
        qb.push_bind(min_count.max(1));
    }

    qb.push(" ORDER BY COUNT(*) DESC, l.pin_code ASC, l.city_id ASC LIMIT ");
    qb.push_bind(page.limit());
    qb.push(") g JOIN cities c ON c.id = g.city_id ORDER BY g.count DESC, g.pin_code ASC, g.city_id ASC");

    let rows: Vec<(String, uuid::Uuid, String, f64, f64, i64)> = qb
        .build_query_as()
        .fetch_all(&state.db)
        .await
        .map_err(|e: sqlx::Error| AppError::Internal(anyhow::anyhow!(e)))?;

    Ok(Json(
        rows.into_iter()
            .map(
                |(pin_code, city_id, city_name, lat, lng, count)| CoveragePoint {
                    pin_code,
                    city_id,
                    city_name,
                    lat,
                    lng,
                    count,
                },
            )
            .collect(),
    ))
}
