use crate::infrastructure::repositories::sqlx_lettering_repository::LetteringRow;
use crate::presentation::http::pagination::{TimePosition, continuation, decode_cursor};
use crate::{
    domain::lettering::entity::Lettering,
    presentation::http::contracts::{GalleryResponse, PublicLettering},
    presentation::http::{errors::AppError, state::AppState},
};
use axum::{
    Extension, Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, QueryBuilder};
use std::time::Instant;
use tracing::{debug, error, info, instrument};
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
    Extension(revision): Extension<
        crate::presentation::http::middleware::public_revision::PublicRevision,
    >,
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

    let cache_key = revision.key(&generate_cache_key(&params, safe_limit));
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
                 FROM discoverable_letterings l",
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
                        l.location_source, l.location_error, l.location_name, l.location_accuracy_m, ST_AsText(l.location) AS location_wkt
                 FROM discoverable_letterings l",
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
                    .map(|row| Lettering::try_from(row).map(Into::into))
                    .collect::<Result<_, _>>()?;

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
