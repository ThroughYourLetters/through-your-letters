use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    domain::lettering::repository::LetteringRepository,
    presentation::http::{
        contracts::{LetteringDetailResponse, PublicLettering},
        errors::AppError,
        middleware::{
            rate_limit::enforce,
            user::{decode_optional_user_claims, decode_required_user_claims},
        },
        state::AppState,
    },
};

pub async fn get_lettering(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<LetteringDetailResponse>, AppError> {
    let requester_user_id = decode_optional_user_claims(&headers, &state.config.jwt_secret)
        .and_then(|claims| Uuid::parse_str(&claims.sub).ok());
    let lettering = state
        .lettering_repo
        .find_readable_by_id(id, requester_user_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Lettering not found".to_string()))?;

    let owner_user_id: Option<Uuid> =
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT user_id FROM letterings WHERE id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

    let is_owner = owner_user_id
        .and_then(|owner| requester_user_id.map(|requester| requester == owner))
        .unwrap_or(false);

    Ok(Json(LetteringDetailResponse {
        lettering: lettering.into(),
        is_owner,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ContributorQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

pub async fn get_contributor_letterings(
    State(state): State<AppState>,
    Path(tag): Path<String>,
    Query(params): Query<ContributorQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let count = state
        .lettering_repo
        .count_by_contributor(&tag)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let letterings = state
        .lettering_repo
        .find_by_contributor(&tag, params.limit.clamp(1, 100), params.offset.max(0))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let letterings: Vec<PublicLettering> = letterings.into_iter().map(Into::into).collect();
    Ok(Json(serde_json::json!({
        "contributor_tag": tag,
        "total_count": count,
        "letterings": letterings,
    })))
}

pub async fn get_similar(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let requester = decode_optional_user_claims(&headers, &state.config.jwt_secret)
        .and_then(|claims| Uuid::parse_str(&claims.sub).ok());
    // Fetch the source lettering's metadata
    let source: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT ml_style, ml_script, pin_code FROM readable_letterings($2) WHERE id = $1",
    )
    .bind(id)
    .bind(requester)
    .fetch_optional(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    let Some((ml_style, ml_script, pin_code)) = source else {
        return Err(AppError::NotFound("Lettering not found".into()));
    };

    // Find similar by matching style, script, or pin_code (excluding self)
    type SimilarRow = (
        Uuid,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let rows: Vec<SimilarRow> = sqlx::query_as(
        r#"SELECT id, image_url, thumbnail_small, detected_text, ml_style, ml_script
           FROM letterings
           WHERE id != $1 AND id IN (SELECT id FROM discoverable_letterings)
             AND (ml_style = $2 OR ml_script = $3 OR pin_code = $4)
           ORDER BY
             CASE WHEN ml_style = $2 AND ml_script = $3 THEN 0
                  WHEN ml_style = $2 THEN 1
                  WHEN ml_script = $3 THEN 2
                  ELSE 3 END,
             created_at DESC
           LIMIT 6"#,
    )
    .bind(id)
    .bind(&ml_style)
    .bind(&ml_script)
    .bind(&pin_code)
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    let similar: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(id, image_url, thumbnail, detected_text, style, script)| {
            serde_json::json!({
                "id": id,
                "image_url": image_url,
                "thumbnail": thumbnail,
                "detected_text": detected_text,
                "ml_style": style,
                "ml_script": script,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "similar": similar })))
}

pub async fn download_lettering(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Redirect, AppError> {
    let requester = decode_optional_user_claims(&headers, &state.config.jwt_secret)
        .and_then(|claims| Uuid::parse_str(&claims.sub).ok());
    let lettering = state
        .lettering_repo
        .find_readable_by_id(id, requester)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Lettering not found".to_string()))?;

    Ok(Redirect::temporary(&lettering.image_url))
}

#[derive(Debug, Deserialize)]
pub struct ReportRequest {
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct LinkRevisitRequest {
    pub revisit_lettering_id: Uuid,
    pub notes: Option<String>,
}

pub async fn delete_lettering(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    // Single query: get image_url and owner in one round-trip
    let row: Option<(String, Option<Uuid>)> =
        sqlx::query_as("SELECT image_url, user_id FROM letterings WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

    let (image_url, owner_user_id) =
        row.ok_or_else(|| AppError::NotFound("Lettering not found".to_string()))?;

    let owner_id = owner_user_id.ok_or_else(|| {
        AppError::Forbidden(
            "This upload cannot be self-deleted because it is not linked to a user account"
                .to_string(),
        )
    })?;

    let requester_user_id = decode_optional_user_claims(&headers, &state.config.jwt_secret)
        .and_then(|claims| Uuid::parse_str(&claims.sub).ok());
    if requester_user_id != Some(owner_id) {
        return Err(AppError::Forbidden(
            "You can only delete your own uploads".to_string(),
        ));
    }

    // Delete from Cloudflare R2 — matches paths written by upload.rs
    let url_parts: Vec<&str> = image_url.split('/').collect();
    if let Some(filename) = url_parts.last() {
        let key = format!("letterings/{}", filename);
        if state.storage.delete(&key).await.is_err() {
            tracing::error!(lettering_id = %id, "Failed to delete R2 object");
        }
        let _ = state.storage.delete(&format!("thumbs/{}", filename)).await;
    }

    // Delete from database (cascades to likes, comments)
    state
        .lettering_repo
        .delete(id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    tracing::info!(lettering_id = %id, "Lettering deleted successfully");

    Ok(StatusCode::NO_CONTENT)
}

/// Report an artifact. Requires authentication. Increments report_count and appends the reason.
/// Items crossing the threshold (3 reports) are automatically hidden (REPORTED status).
pub async fn report_lettering(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<ReportRequest>,
) -> Result<StatusCode, AppError> {
    // Require authentication to prevent anonymous report-flooding
    let claims = decode_required_user_claims(&headers, &state.config.jwt_secret)?;

    let reason = body.reason.trim().to_string();
    if reason.is_empty() {
        return Err(AppError::BadRequest(
            "Report reason is required".to_string(),
        ));
    }
    if reason.len() > 500 {
        return Err(AppError::BadRequest(
            "Report reason must be 500 characters or less".to_string(),
        ));
    }

    enforce(
        &state,
        &format!("report:{id}:{}", claims.sub),
        &state.config.limits.security.report,
    )
    .await?;

    // Append the reason string as a JSONB element (not a nested array)
    let result = sqlx::query(
        r#"UPDATE letterings
        SET report_count = report_count + 1,
            report_reasons = report_reasons || jsonb_build_array($2::text),
            status = CASE WHEN report_count + 1 >= 3 THEN 'REPORTED' ELSE status END,
            updated_at = NOW()
        WHERE id = $1"#,
    )
    .bind(id)
    .bind(&reason)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Lettering not found".to_string()));
    }

    tracing::info!(lettering_id = %id, user = %claims.sub, "Lettering reported");
    Ok(StatusCode::CREATED)
}

pub async fn link_revisit(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<LinkRevisitRequest>,
) -> Result<StatusCode, AppError> {
    let claims = decode_required_user_claims(&headers, &state.config.jwt_secret)?;
    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Forbidden("Invalid token subject".to_string()))?;

    // Caller must own at least the original lettering
    let owner: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM letterings WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?
        .flatten();

    if owner != Some(user_id) {
        return Err(AppError::Forbidden(
            "You can only link revisits to your own letterings".to_string(),
        ));
    }

    let result = sqlx::query(
        r#"INSERT INTO location_revisits (id, original_lettering_id, revisit_lettering_id, notes)
           SELECT $1, $2, r.id, $4 FROM readable_letterings($5) r WHERE r.id = $3
           ON CONFLICT (original_lettering_id, revisit_lettering_id) DO NOTHING"#,
    )
    .bind(Uuid::now_v7())
    .bind(id)
    .bind(body.revisit_lettering_id)
    .bind(body.notes)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    if result.rows_affected() == 0 {
        let readable: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM readable_letterings($2) WHERE id = $1)",
        )
        .bind(body.revisit_lettering_id)
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
        if !readable {
            return Err(AppError::NotFound("Lettering not found".into()));
        }
    }

    Ok(StatusCode::CREATED)
}

#[derive(sqlx::FromRow)]
struct RevisitRow {
    id: Uuid,
    original_lettering_id: Uuid,
    revisit_lettering_id: Uuid,
    notes: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    original_image_url: String,
    original_created_at: chrono::DateTime<chrono::Utc>,
    revisit_image_url: String,
    revisit_created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn get_revisits(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let requester = decode_optional_user_claims(&headers, &state.config.jwt_secret)
        .and_then(|claims| Uuid::parse_str(&claims.sub).ok());
    let rows = sqlx::query_as::<_, RevisitRow>(
        r#"SELECT
                lr.id,
                lr.original_lettering_id,
                lr.revisit_lettering_id,
                lr.notes,
                lr.created_at,
                o.image_url as original_image_url,
                o.created_at as original_created_at,
                r.image_url as revisit_image_url,
                r.created_at as revisit_created_at
           FROM location_revisits lr
           JOIN readable_letterings($2) o ON o.id = lr.original_lettering_id
           JOIN readable_letterings($2) r ON r.id = lr.revisit_lettering_id
           WHERE lr.original_lettering_id = $1 OR lr.revisit_lettering_id = $1
           ORDER BY lr.created_at DESC"#,
    )
    .bind(id)
    .bind(requester)
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    let revisits: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "original_lettering_id": row.original_lettering_id,
                "revisit_lettering_id": row.revisit_lettering_id,
                "notes": row.notes,
                "created_at": row.created_at,
                "original": {
                    "image_url": row.original_image_url,
                    "created_at": row.original_created_at
                },
                "revisit": {
                    "image_url": row.revisit_image_url,
                    "created_at": row.revisit_created_at
                }
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "revisits": revisits })))
}
