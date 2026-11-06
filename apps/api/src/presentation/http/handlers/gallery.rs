use crate::presentation::http::pagination::{TimePosition, continuation, decode_cursor};
use crate::{
    domain::lettering::entity::Lettering,
    presentation::http::contracts::{GalleryResponse, PublicLettering},
    presentation::http::{errors::AppError, state::AppState},
};
use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, QueryBuilder};
use std::time::Instant;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Sort {
    #[default]
    Newest,
    Oldest,
    Popular,
}

/// Gallery requests support either a continuation cursor or a legacy offset.
#[derive(Debug, Deserialize)]
pub struct GalleryQuery {
    limit: Option<i64>,
    #[serde(default)]
    offset: i64,
    after: Option<String>,
    city_id: Option<Uuid>,
    script: Option<String>,
    style: Option<String>,
    #[serde(default)]
    sort_by: Sort,
}

impl GalleryQuery {
    fn selection(&self) -> serde_json::Value {
        let normalized = crate::presentation::http::public_filters::normalized;
        serde_json::json!([
            self.city_id,
            normalized(self.script.as_deref()),
            normalized(self.style.as_deref()),
            self.sort_by
        ])
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GalleryPosition {
    time: TimePosition,
    likes: i32,
}

const GALLERY_CACHE_PREFIX: &str = "gallery:";

fn apply_gallery_filters<'a>(qb: &mut QueryBuilder<'a, Postgres>, params: &'a GalleryQuery) {
    crate::presentation::http::public_filters::apply_public_filters(
        qb,
        params.city_id,
        params.script.as_deref(),
        params.style.as_deref(),
    );
}

fn generate_cache_key(params: &GalleryQuery, limit: i64) -> String {
    // Structured keys cannot collide when user-entered filters contain colons.
    format!(
        "{}{}",
        GALLERY_CACHE_PREFIX,
        serde_json::json!([limit, params.offset, params.selection(), params.after,])
    )
}

#[instrument(skip_all, fields(
    limit = ?params.limit,
    offset = params.offset,
    city_id = ?params.city_id,
    has_filters = !(params.script.is_none() && params.style.is_none())
))]
pub async fn get_letterings(
    State(state): State<AppState>,
    Query(params): Query<GalleryQuery>,
) -> Result<Json<GalleryResponse>, AppError> {
    let start_time = Instant::now();

    let page = crate::presentation::http::pagination::Page::new(
        params
            .limit
            .unwrap_or(state.config.limits.gallery.default_limit),
        params.offset,
        state.config.limits.gallery.max_limit,
        &state.config.limits.pagination,
    )?;
    let safe_limit = page.limit();
    let safe_offset = page.offset();
    if params.after.is_some() && safe_offset != 0 {
        return Err(AppError::BadRequest(
            "Use a cursor or an offset, not both".into(),
        ));
    }
    let scope = format!(
        "gallery:{:x}",
        Sha256::digest(params.selection().to_string().as_bytes())
    );
    let after: Option<GalleryPosition> = decode_cursor(
        params.after.as_deref(),
        &scope,
        &state.config.limits.pagination,
    )?;

    debug!(
        "Processing gallery request with limit={}, offset={}",
        safe_limit, safe_offset
    );

    let cache_key = generate_cache_key(&params, safe_limit);
    let db = state.db.clone();

    let response = state
        .cache
        .get_or_fetch(
            &cache_key,
            state.config.limits.cache.ttl_seconds,
            || async move {
                // Count query
                let mut count_qb = QueryBuilder::<Postgres>::new(
                    "SELECT COUNT(*)::bigint
                 FROM letterings l",
                );
                apply_gallery_filters(&mut count_qb, &params);

                let total: i64 =
                    count_qb
                        .build_query_scalar()
                        .fetch_one(&db)
                        .await
                        .map_err(|error| {
                            anyhow::anyhow!(error).context("Gallery count query failed")
                        })?;

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
                        l.location_source, l.location_error, ST_AsText(l.location) AS location
                 FROM letterings l",
                );
                apply_gallery_filters(&mut data_qb, &params);

                if let Some(after) = after {
                    match params.sort_by {
                        Sort::Newest | Sort::Oldest => {
                            data_qb
                                .push(" AND (l.created_at,l.id) ")
                                .push(if matches!(params.sort_by, Sort::Oldest) {
                                    ">"
                                } else {
                                    "<"
                                })
                                .push(" (")
                                .push_bind(after.time.created_at)
                                .push(",")
                                .push_bind(after.time.id)
                                .push(")");
                        }
                        Sort::Popular => {
                            data_qb
                                .push(" AND (l.likes_count,l.created_at,l.id) < (")
                                .push_bind(after.likes)
                                .push(",")
                                .push_bind(after.time.created_at)
                                .push(",")
                                .push_bind(after.time.id)
                                .push(")");
                        }
                    }
                }
                let order_by = match params.sort_by {
                    Sort::Oldest => " ORDER BY l.created_at ASC, l.id ASC",
                    Sort::Popular => " ORDER BY l.likes_count DESC, l.created_at DESC, l.id DESC",
                    Sort::Newest => " ORDER BY l.created_at DESC, l.id DESC",
                };

                data_qb
                    .push(order_by)
                    .push(" LIMIT ")
                    .push_bind(safe_limit + 1)
                    .push(" OFFSET ")
                    .push_bind(safe_offset);

                let mut rows: Vec<LetteringRow> = data_qb
                    .build_query_as()
                    .fetch_all(&db)
                    .await
                    .map_err(|error| anyhow::anyhow!(error).context("Gallery data query failed"))?;

                let next_cursor = continuation(&mut rows, page, &scope, |row| GalleryPosition {
                    time: TimePosition {
                        created_at: row.created_at,
                        id: row.id,
                    },
                    likes: row.likes_count,
                })?;
                let letterings: Vec<PublicLettering> = rows
                    .into_iter()
                    .map(Lettering::from)
                    .map(Into::into)
                    .collect();

                Ok(GalleryResponse {
                    total,
                    letterings,
                    limit: safe_limit,
                    offset: safe_offset,
                    next_cursor,
                })
            },
        )
        .await
        .map_err(|e| {
            error!("Gallery fetch failed");
            AppError::Internal(anyhow::anyhow!(e))
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
    location_source: crate::domain::lettering::entity::LocationSource,
    location_error: Option<String>,
}

impl From<LetteringRow> for Lettering {
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
                if (-180.0..=180.0).contains(&lng) && (-90.0..=90.0).contains(&lat) {
                    Some(vec![lng, lat])
                } else {
                    warn!("Invalid coordinates in stored lettering");
                    None
                }
            })
            .map(|coordinates| Coordinates {
                r#type: "Point".into(),
                coordinates,
            });

        // Map database status to domain enum with logging for unknown values
        let status = match r.status.as_str() {
            "APPROVED" => LetteringStatus::Approved,
            "REJECTED" => LetteringStatus::Rejected,
            "REPORTED" => LetteringStatus::Reported,
            "PENDING" => LetteringStatus::Pending,
            _ => {
                warn!(lettering_id = %r.id, "Unknown lettering status; defaulting to pending");
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
            location: coords,
            location_source: r.location_source,
            location_error: r.location_error,
            pin_code: r.pin_code,
            detected_text: r.detected_text,
            ml_metadata: Some(ImageMetadata {
                style: r.ml_style,
                script: r.ml_script,
                confidence: r.ml_confidence,
                color_palette: r.ml_color_palette.and_then(|v| {
                    serde_json::from_value(v)
                        .inspect_err(|_| {
                            warn!(lettering_id = %r.id, "Failed to parse stored color palette");
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
                .inspect_err(|_| {
                    warn!(lettering_id = %r.id, "Failed to parse stored report reasons");
                })
                .unwrap_or_default(),
            cultural_context: r.cultural_context,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}
