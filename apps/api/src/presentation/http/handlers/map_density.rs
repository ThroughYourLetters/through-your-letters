//! Bounded density cells and paginated uploads for the visible map.
use crate::presentation::http::public_filters::{apply_public_filters, normalized};
use crate::presentation::http::{errors::AppError, state::AppState};
use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};
use uuid::Uuid;

// Web Mercator cannot represent the poles.
const EDGE: f64 = 20_037_508.342_789_244;
const MAX_LAT: f64 = 85.051_128_779_806_6;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeoQuery {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
    pub zoom: Option<u32>,
    pub after: Option<Uuid>,
    pub city_id: Option<Uuid>,
    pub script: Option<String>,
    pub style: Option<String>,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Filters {
    pub city_id: Option<Uuid>,
    pub script: Option<String>,
    pub style: Option<String>,
}

const JOIN: &str = " FROM letterings l";

fn mercator(lat: f64) -> f64 {
    ((std::f64::consts::PI / 4.0) + (lat.clamp(-MAX_LAT, MAX_LAT).to_radians() / 2.0))
        .tan()
        .ln()
        * EDGE
        / std::f64::consts::PI
}
fn latitude(y: f64) -> f64 {
    if y <= -EDGE {
        return -MAX_LAT;
    }
    if y >= EDGE {
        return MAX_LAT;
    }
    (2.0 * (y * std::f64::consts::PI / EDGE).exp().atan() - std::f64::consts::FRAC_PI_2)
        .to_degrees()
        .clamp(-MAX_LAT, MAX_LAT)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}
impl GeoQuery {
    fn validate(&self) -> Result<(), AppError> {
        if ![self.west, self.south, self.east, self.north]
            .iter()
            .all(|v| v.is_finite())
            || !(-180.0..=180.0).contains(&self.west)
            || !(-180.0..=180.0).contains(&self.east)
            || !(-MAX_LAT..=MAX_LAT).contains(&self.south)
            || !(-MAX_LAT..=MAX_LAT).contains(&self.north)
            || self.south >= self.north
            || self.west == self.east
            || (self.west == 180.0 && self.east == -180.0)
            || self.zoom.is_some_and(|z| z > 22)
        {
            return Err(AppError::BadRequest(
                "Enter valid map bounds and zoom".into(),
            ));
        }
        Ok(())
    }
    fn bounds(&self) -> Vec<Bounds> {
        let b = |west, east| Bounds {
            west,
            south: self.south,
            east,
            north: self.north,
        };
        if self.west < self.east {
            vec![b(self.west, self.east)]
        } else {
            vec![b(self.west, 180.0), b(-180.0, self.east)]
        }
    }
}
fn in_bounds(sql: &mut QueryBuilder<'_, Postgres>, bounds: &[Bounds]) {
    sql.push(" AND l.location_source='postal_code' AND (");
    for (index, b) in bounds.iter().enumerate() {
        if index > 0 {
            sql.push(" OR ");
        }
        sql.push("(l.location::geometry && ST_MakeEnvelope(")
            .push_bind(b.west)
            .push(",")
            .push_bind(b.south)
            .push(",")
            .push_bind(b.east)
            .push(",")
            .push_bind(b.north)
            .push(",4326) AND ST_X(l.location::geometry) >= ")
            .push_bind(b.west)
            .push(" AND ST_X(l.location::geometry) ")
            .push(if b.east == 180.0 { "<= " } else { "< " })
            .push_bind(b.east)
            .push(" AND ST_Y(l.location::geometry) >= ")
            .push_bind(b.south)
            .push(" AND ST_Y(l.location::geometry) ")
            .push(if b.north >= MAX_LAT { "<= " } else { "< " })
            .push_bind(b.north)
            .push(")");
    }
    sql.push(")");
}
async fn transaction(state: &AppState) -> Result<sqlx::Transaction<'_, Postgres>, AppError> {
    let mut tx = state.db.begin().await?;
    sqlx::query("SELECT set_config('statement_timeout',$1,true)")
        .bind(state.config.limits.map.query_timeout_ms.to_string())
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}
fn query_error(error: sqlx::Error) -> AppError {
    if error
        .as_database_error()
        .is_some_and(|e| e.code().as_deref() == Some("57014"))
    {
        AppError::ExternalService("Map query timed out".into())
    } else {
        error.into()
    }
}

#[derive(FromRow)]
struct CellRow {
    cell_x: i64,
    cell_y: i64,
    x: f64,
    y: f64,
    count: i64,
    thumbnail: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct DensityCell {
    pub id: String,
    pub lat: f64,
    pub lng: f64,
    pub count: i64,
    pub thumbnail: String,
    pub bounds: Bounds,
}
#[derive(Serialize, Deserialize)]
pub struct Density {
    pub cells: Vec<DensityCell>,
    pub truncated: bool,
}

fn grid(query: &GeoQuery, max_cells: u32, pixels: u32, zoom: u32) -> f64 {
    let mut size = (2.0 * EDGE / (2.0_f64.powi(zoom as i32) * 256.0 / pixels as f64)).min(EDGE);
    loop {
        let rows = ((mercator(query.north) - mercator(query.south)) / size).ceil() + 2.0;
        let columns: f64 = query
            .bounds()
            .iter()
            .map(|b| ((b.east - b.west) * EDGE / 180.0 / size).ceil() + 2.0)
            .sum();
        if rows * columns <= max_cells as f64 || size >= EDGE {
            return size;
        }
        size *= 2.0;
    }
}
fn longitude(x: f64) -> f64 {
    (x * 180.0 / EDGE).clamp(-180.0, 180.0)
}
fn cell_start(value: f64, size: f64) -> f64 {
    ((value + EDGE) / size).floor() * size - EDGE
}
fn aligned(bounds: Vec<Bounds>, size: f64) -> Vec<Bounds> {
    bounds
        .into_iter()
        .map(|b| Bounds {
            west: longitude(cell_start(b.west * EDGE / 180.0, size)),
            east: longitude(cell_start(b.east * EDGE / 180.0, size) + size),
            south: latitude(cell_start(mercator(b.south), size)),
            north: latitude(cell_start(mercator(b.north), size) + size),
        })
        .collect()
}
fn cell_bounds(x: i64, y: i64, size: f64) -> Bounds {
    Bounds {
        west: longitude(x as f64 * size - EDGE),
        east: longitude((x + 1) as f64 * size - EDGE),
        south: latitude(y as f64 * size - EDGE),
        north: latitude((y + 1) as f64 * size - EDGE),
    }
}
async fn fetch_density(
    state: &AppState,
    query: &GeoQuery,
    size: f64,
    bounds: &[Bounds],
) -> anyhow::Result<Density> {
    let mut sql = QueryBuilder::<Postgres>::new("WITH settings AS (SELECT ");
    sql.push_bind(size).push("::double precision AS size,").push_bind(EDGE).push("::double precision AS edge,").push_bind((2.0*EDGE/size).ceil()-1.0).push("::double precision AS last), points AS (SELECT ST_X(ST_Transform(l.location::geometry,3857)) AS x, ST_Y(ST_Transform(l.location::geometry,3857)) AS y, COALESCE(l.thumbnail_small,'') AS thumbnail");
    sql.push(JOIN);
    apply_public_filters(
        &mut sql,
        query.city_id,
        query.script.as_deref(),
        query.style.as_deref(),
    );
    in_bounds(&mut sql, bounds);
    sql.push(
        "), bins AS (
        SELECT GREATEST(0,LEAST(FLOOR((x+edge)/size),last))::bigint AS cell_x,
               GREATEST(0,LEAST(FLOOR((y+edge)/size),last))::bigint AS cell_y,x,y,thumbnail
        FROM points CROSS JOIN settings)
        SELECT cell_x,cell_y,AVG(x)::double precision AS x,
               AVG(y)::double precision AS y,COUNT(*)::bigint AS count,MAX(thumbnail) AS thumbnail
        FROM bins GROUP BY cell_x,cell_y
        ORDER BY count DESC,cell_x,cell_y LIMIT ",
    )
    .push_bind(i64::from(state.config.limits.map.max_cells) + 1);
    let mut tx = transaction(state).await?;
    let mut rows: Vec<CellRow> = sql
        .build_query_as()
        .fetch_all(&mut *tx)
        .await
        .map_err(query_error)?;
    tx.commit().await?;
    let truncated = rows.len() > state.config.limits.map.max_cells as usize;
    rows.truncate(state.config.limits.map.max_cells as usize);
    let cells = rows
        .into_iter()
        .map(|r| DensityCell {
            id: format!("{size}:{}:{}", r.cell_x, r.cell_y),
            lat: latitude(r.y),
            lng: longitude(r.x),
            count: r.count,
            thumbnail: r.thumbnail,
            bounds: cell_bounds(r.cell_x, r.cell_y, size),
        })
        .collect();
    Ok(Density { cells, truncated })
}

pub async fn density(
    State(state): State<AppState>,
    Query(query): Query<GeoQuery>,
) -> Result<Json<Density>, AppError> {
    query.validate()?;
    let zoom = query
        .zoom
        .ok_or_else(|| AppError::BadRequest("Zoom is required".into()))?;
    let size = grid(
        &query,
        state.config.limits.map.max_cells,
        state.config.limits.map.grid_pixels,
        zoom,
    );
    let bounds = aligned(query.bounds(), size);
    let key = format!(
        "density:{}",
        serde_json::json!([
            bounds,
            size,
            query.city_id,
            normalized(query.script.as_deref()),
            normalized(query.style.as_deref())
        ])
    );
    let response = state
        .cache
        .get_or_fetch(&key, state.config.limits.cache.ttl_seconds, || {
            fetch_density(&state, &query, size, &bounds)
        })
        .await
        .map_err(|e| AppError::ExternalService(e.to_string()))?;
    Ok(Json(response))
}

#[derive(Serialize, Deserialize)]
pub struct Summary {
    pub unmapped_count: i64,
}
pub async fn summary(
    State(state): State<AppState>,
    Query(filters): Query<Filters>,
) -> Result<Json<Summary>, AppError> {
    let key = format!(
        "map-summary:{}",
        serde_json::json!([
            filters.city_id,
            normalized(filters.script.as_deref()),
            normalized(filters.style.as_deref())
        ])
    );
    let response = state
        .cache
        .get_or_fetch(&key, state.config.limits.cache.ttl_seconds, || async {
            let mut sql = QueryBuilder::<Postgres>::new("SELECT COUNT(*)::bigint");
            sql.push(JOIN);
            apply_public_filters(
                &mut sql,
                filters.city_id,
                filters.script.as_deref(),
                filters.style.as_deref(),
            );
            sql.push(" AND (l.location_source <> 'postal_code' OR l.location IS NULL OR ABS(ST_Y(l.location::geometry)) > ").push_bind(MAX_LAT).push(")");
            let mut tx = transaction(&state).await.map_err(anyhow::Error::new)?;
            let unmapped_count = sql
                .build_query_scalar()
                .fetch_one(&mut *tx)
                .await
                .map_err(query_error)?;
            tx.commit().await?;
            Ok(Summary { unmapped_count })
        })
        .await
        .map_err(|e| AppError::ExternalService(e.to_string()))?;
    Ok(Json(response))
}

#[derive(Serialize, Deserialize, FromRow)]
pub struct Photo {
    pub id: Uuid,
    pub thumbnail: String,
    pub pin_code: String,
    pub lat: f64,
    pub lng: f64,
}
#[derive(Serialize)]
pub struct Photos {
    pub items: Vec<Photo>,
    pub next_cursor: Option<Uuid>,
}
pub async fn photos(
    State(state): State<AppState>,
    Query(query): Query<GeoQuery>,
) -> Result<Json<Photos>, AppError> {
    query.validate()?;
    let mut sql = QueryBuilder::<Postgres>::new(
        "SELECT l.id,COALESCE(l.thumbnail_small,'') AS thumbnail,l.pin_code,ST_Y(l.location::geometry) AS lat,ST_X(l.location::geometry) AS lng",
    );
    sql.push(JOIN);
    apply_public_filters(
        &mut sql,
        query.city_id,
        query.script.as_deref(),
        query.style.as_deref(),
    );
    in_bounds(&mut sql, &query.bounds());
    if let Some(after) = query.after {
        sql.push(" AND l.id > ").push_bind(after);
    }
    sql.push(" ORDER BY l.id LIMIT ")
        .push_bind(state.config.limits.web.gallery_page_size + 1);
    let mut tx = transaction(&state).await?;
    let mut items: Vec<Photo> = sql
        .build_query_as()
        .fetch_all(&mut *tx)
        .await
        .map_err(query_error)?;
    tx.commit().await?;
    let next_cursor = if items.len() > state.config.limits.web.gallery_page_size as usize {
        items.pop();
        items.last().map(|item| item.id)
    } else {
        None
    };
    Ok(Json(Photos { items, next_cursor }))
}
