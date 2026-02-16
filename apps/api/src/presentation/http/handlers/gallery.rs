use crate::{
    application::get_letterings::dto::PaginatedResponse,
    domain::lettering::entity::Lettering,
    presentation::http::{errors::AppError, state::AppState},
};
use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use serde::Deserialize;
use sqlx::{Postgres, QueryBuilder};
use std::time::Instant;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

/// Query parameters for gallery endpoint with validation and defaults.
///
/// Supports pagination, filtering, and sorting of approved lettering entities.
/// All parameters are optional with sensible defaults for discoverability.
#[derive(Debug, Deserialize)]
pub struct GalleryQuery {
    /// Maximum number of results to return (1-100, default 50)
    #[serde(default = "default_limit")]
    limit: i64,

    /// Number of results to skip for pagination (default 0)
    #[serde(default)]
    offset: i64,

    /// Filter by specific city/region UUID (optional)
    city_id: Option<Uuid>,

    /// Filter by detected script type (e.g., "latin", "devanagari") (optional)
    script: Option<String>,

    /// Filter by visual style category (e.g., "modern", "traditional") (optional)
    style: Option<String>,

    /// Sort order: "newest" (default), "oldest", "popular" (optional)
    sort_by: Option<String>,
}

/// Default pagination limit for gallery queries.
/// Balances performance with user experience for typical browsing patterns.
fn default_limit() -> i64 {
    50
}

/// Maximum allowed pagination limit to prevent resource exhaustion.
const MAX_LIMIT: i64 = 100;

/// Cache key prefix for gallery queries.
const GALLERY_CACHE_PREFIX: &str = "gallery:";

/// Cache TTL for gallery results in seconds (5 minutes).
const GALLERY_CACHE_TTL: usize = 300;

/// Applies filter conditions to gallery query based on provided parameters.
///
/// Ensures only approved letterings from discoverable regions are included,
/// with optional filtering by location, visual characteristics, or content type.
///
/// # Arguments
/// * `qb` - Query builder to modify with filter conditions
/// * `params` - User-provided filter parameters
fn apply_gallery_filters(qb: &mut QueryBuilder<'_, Postgres>, params: &GalleryQuery) {
    // Base filters: only approved letterings from discoverable regions
    qb.push(
        " WHERE l.status = 'APPROVED'
          AND COALESCE(rp.discoverability_enabled, true)",
    );

    // Optional city/region filter
    if let Some(city_id) = params.city_id {
        debug!("Filtering by city_id: {}", city_id);
        qb.push(" AND l.city_id = ").push_bind(city_id);
    }

    // Optional script type filter (with sanitization)
    if let Some(script) = params
        .script
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        debug!("Filtering by script: {}", script);
        qb.push(" AND l.ml_script = ").push_bind(script.to_string());
    }

    // Optional visual style filter (with sanitization)
    if let Some(style) = params
        .style
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        debug!("Filtering by style: {}", style);
        qb.push(" AND l.ml_style = ").push_bind(style.to_string());
    }
}

/// Generates cache key for gallery query results.
///
/// Creates a deterministic key based on all query parameters to enable
/// efficient caching and cache invalidation.
fn generate_cache_key(params: &GalleryQuery) -> String {
    format!(
        "{}{}:{}:{}:{}:{}:{}",
        GALLERY_CACHE_PREFIX,
        params.limit,
        params.offset,
        params
            .city_id
            .map(|u| u.to_string())
            .unwrap_or_else(|| "all".to_string()),
        params.script.as_deref().unwrap_or("all"),
        params.style.as_deref().unwrap_or("all"),
        params.sort_by.as_deref().unwrap_or("newest")
    )
}

/// Retrieves a paginated list of approved letterings with optional filtering.
///
/// This endpoint serves as the primary discovery mechanism for lettering content.
/// It supports pagination, geographic filtering, visual characteristic filtering,
/// and multiple sort orders. Results are cached to improve performance.
///
/// # Query Parameters
/// - `limit`: Number of results (1-100, default 50)
/// - `offset`: Pagination offset (default 0)
/// - `city_id`: Filter by city UUID (optional)
/// - `script`: Filter by script type (optional)
/// - `style`: Filter by visual style (optional)
/// - `sort_by`: Sort order - "newest", "oldest", "popular" (optional)
///
/// # Returns
/// Paginated response containing lettering entities and metadata
///
/// # Errors
/// Returns `AppError::Internal` for database connectivity issues
/// or `AppError::BadRequest` for invalid parameters
#[instrument(skip(state), fields(
    limit = params.limit,
    offset = params.offset,
    city_id = ?params.city_id,
    has_filters = !(params.script.is_none() && params.style.is_none())
))]
pub async fn get_letterings(
    State(state): State<AppState>,
    Query(params): Query<GalleryQuery>,
    headers: HeaderMap,
) -> Result<Json<PaginatedResponse>, AppError> {
    let start_time = Instant::now();

    // Validate and sanitize input parameters
    let safe_limit = params.limit.clamp(1, MAX_LIMIT);
    let safe_offset = params.offset.max(0);

    if safe_limit != params.limit {
        warn!(
            "Gallery limit clamped from {} to {}",
            params.limit, safe_limit
        );
    }

    debug!(
        "Processing gallery request with limit={}, offset={}",
        safe_limit, safe_offset
    );

    let cache_key = generate_cache_key(&params);
    let db = state.db.clone();

    let response = state
        .cache
        .get_or_fetch(&cache_key, GALLERY_CACHE_TTL as u64, || async move {
            // Count query
            let mut count_qb = QueryBuilder::<Postgres>::new(
                "SELECT COUNT(*)::bigint
                 FROM letterings l
                 JOIN cities c ON c.id = l.city_id
                 LEFT JOIN region_policies rp ON rp.country_code = c.country_code",
            );
            apply_gallery_filters(&mut count_qb, &params);

            let total: i64 = count_qb
                .build_query_scalar()
                .fetch_one(&db)
                .await
                .map_err(|e| anyhow::anyhow!("Gallery count query failed: {}", e))?;

            debug!("Gallery query found {} total matching letterings", total);

            // Data query
            let mut data_qb = QueryBuilder::<Postgres>::new(
                "SELECT l.id, l.city_id, l.contributor_tag, l.image_url,
                        l.thumbnail_small, l.thumbnail_medium, l.thumbnail_large,
                        l.pin_code, l.status, l.created_at, l.updated_at,
                        l.detected_text, l.description, l.image_hash,
                        l.ml_style, l.ml_script, l.ml_confidence, l.ml_color_palette,
                        l.cultural_context, l.report_count, l.report_reasons,
                        l.likes_count, l.comments_count, l.uploaded_by_ip,
                        ST_AsText(l.location) AS location
                 FROM letterings l
                 JOIN cities c ON c.id = l.city_id
                 LEFT JOIN region_policies rp ON rp.country_code = c.country_code",
            );
            apply_gallery_filters(&mut data_qb, &params);

            let order_by = match params.sort_by.as_deref() {
                Some("oldest") => " ORDER BY l.created_at ASC",
                Some("popular") => " ORDER BY l.likes_count DESC, l.created_at DESC",
                _ => " ORDER BY l.created_at DESC",
            };

            data_qb
                .push(order_by)
                .push(" LIMIT ")
                .push_bind(safe_limit)
                .push(" OFFSET ")
                .push_bind(safe_offset);

            let rows: Vec<LetteringRow> = data_qb
                .build_query_as()
                .fetch_all(&db)
                .await
                .map_err(|e| anyhow::anyhow!("Gallery data query failed: {}", e))?;

            let letterings: Vec<Lettering> = rows.into_iter().map(Into::into).collect();

            Ok(PaginatedResponse {
                total,
                letterings,
                limit: safe_limit,
                offset: safe_offset,
            })
        })
        .await
        .map_err(|e| {
            error!("Gallery fetch failed: {}", e);
            AppError::Internal(format!("Failed to retrieve letterings: {}", e))
        })?;

    let duration = start_time.elapsed();
    info!(
        "Gallery request completed in {:?}, returned {} of {} letterings",
        duration,
        response.letterings.len(),
        response.total
    );

    Ok(Json(response))
}

/// Database row representation for lettering entities from gallery queries.
///
/// Maps directly to database columns with proper type handling for
/// PostgreSQL-specific types like PostGIS geometry and JSONB.
#[derive(sqlx::FromRow)]
struct LetteringRow {
    id: Uuid,
    city_id: Uuid,
    contributor_tag: String,
    image_url: String,
    thumbnail_small: String,
    thumbnail_medium: String,
    thumbnail_large: String,
    pin_code: String,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    detected_text: Option<String>,
    description: Option<String>,
    image_hash: Option<String>,
    ml_style: Option<String>,
    ml_script: Option<String>,
    ml_confidence: Option<f32>,
    ml_color_palette: Option<serde_json::Value>,
    cultural_context: Option<String>,
    report_count: i32,
    report_reasons: serde_json::Value,
    likes_count: i32,
    comments_count: i32,
    uploaded_by_ip: Option<sqlx::types::ipnetwork::IpNetwork>,
    location: Option<String>,
}

impl From<LetteringRow> for Lettering {
    /// Converts database row to domain entity with proper error handling.
    ///
    /// Handles coordinate parsing, status mapping, and JSON deserialization
    /// with fallback values for robustness in production environments.
    fn from(r: LetteringRow) -> Self {
        use crate::domain::lettering::entity::*;

        // Parse PostGIS POINT geometry with error handling
        let coords = r
            .location
            .as_deref()
            .and_then(|wkt| {
                let wkt = wkt.trim();
                let inner = wkt.strip_prefix("POINT(")?.strip_suffix(')')?;
                let mut parts = inner.split_whitespace();
                let lng: f64 = parts.next()?.parse().ok()?;
                let lat: f64 = parts.next()?.parse().ok()?;

                // Validate coordinate bounds
                if lng >= -180.0 && lng <= 180.0 && lat >= -90.0 && lat <= 90.0 {
                    Some(vec![lng, lat])
                } else {
                    warn!("Invalid coordinates parsed: lng={}, lat={}", lng, lat);
                    None
                }
            })
            .unwrap_or_else(|| {
                warn!("Using fallback coordinates for lettering {}", r.id);
                vec![77.5946, 12.9716] // Default to Bangalore, India
            });

        // Map database status to domain enum with logging for unknown values
        let status = match r.status.as_str() {
            "APPROVED" => LetteringStatus::Approved,
            "REJECTED" => LetteringStatus::Rejected,
            "REPORTED" => LetteringStatus::Reported,
            "PENDING" => LetteringStatus::Pending,
            unknown => {
                warn!(
                    "Unknown lettering status '{}' for ID {}, defaulting to Pending",
                    unknown, r.id
                );
                LetteringStatus::Pending
            }
        };

        Lettering {
            id: r.id,
            city_id: r.city_id,
            contributor_tag: r.contributor_tag,
            image_url: r.image_url,
            thumbnail_urls: ThumbnailUrls {
                small: r.thumbnail_small,
                medium: r.thumbnail_medium,
                large: r.thumbnail_large,
            },
            location: Coordinates {
                r#type: "Point".into(),
                coordinates: coords,
            },
            pin_code: r.pin_code,
            detected_text: r.detected_text,
            ml_metadata: Some(ImageMetadata {
                style: r.ml_style,
                script: r.ml_script,
                confidence: r.ml_confidence,
                color_palette: r.ml_color_palette.and_then(|v| {
                    serde_json::from_value(v)
                        .map_err(|e| {
                            warn!(
                                "Failed to deserialize color palette for lettering {}: {}",
                                r.id, e
                            );
                            e
                        })
                        .ok()
                }),
            }),
            description: r.description,
            is_lettering: true, // Gallery only shows confirmed letterings
            status,
            likes_count: r.likes_count.max(0), // Ensure non-negative
            comments_count: r.comments_count.max(0), // Ensure non-negative
            uploaded_by_ip: r.uploaded_by_ip,
            image_hash: r.image_hash,
            report_count: r.report_count.max(0), // Ensure non-negative
            report_reasons: serde_json::from_value(r.report_reasons)
                .map_err(|e| {
                    warn!(
                        "Failed to deserialize report reasons for lettering {}: {}",
                        r.id, e
                    );
                    e
                })
                .unwrap_or_default(),
            cultural_context: r.cultural_context,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}
