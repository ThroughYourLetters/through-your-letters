use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    domain::lettering::repository::LetteringRepository,
    presentation::http::{
        errors::AppError, middleware::user::decode_optional_user_claims, state::AppState,
    },
};

pub async fn get_lettering(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let lettering = state
        .lettering_repo
        .find_by_id(id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Lettering not found".to_string()))?;

    let owner_user_id: Option<Uuid> =
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT user_id FROM letterings WHERE id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

    let requester_user_id = decode_optional_user_claims(&headers, &state.config.jwt_secret)
        .and_then(|claims| Uuid::parse_str(&claims.sub).ok());

    let is_owner = owner_user_id
        .and_then(|owner| requester_user_id.map(|requester| requester == owner))
        .unwrap_or(false);

    let mut value =
        serde_json::to_value(&lettering).map_err(|e| AppError::Internal(e.to_string()))?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert("is_owner".to_string(), serde_json::Value::Bool(is_owner));
    }

    Ok(Json(value))
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
        .find_by_contributor(&tag, params.limit, params.offset)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "contributor_tag": tag,
        "total_count": count,
        "letterings": letterings,
    })))
}

pub async fn get_similar(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Fetch the source lettering's metadata
    let source: Option<(Option<String>, Option<String>, String)> =
        sqlx::query_as("SELECT ml_style, ml_script, pin_code FROM letterings WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    let Some((ml_style, ml_script, pin_code)) = source else {
        return Ok(Json(serde_json::json!({ "similar": [] })));
    };

    // Find similar by matching style, script, or pin_code (excluding self)
    let rows: Vec<(
        Uuid,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        r#"SELECT id, image_url, thumbnail_small, detected_text, ml_style, ml_script
           FROM letterings
           WHERE id != $1 AND status = 'APPROVED'
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
) -> Result<Redirect, AppError> {
    let lettering = state
        .lettering_repo
        .find_by_id(id)
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
    let lettering = state
        .lettering_repo
        .find_by_id(id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Lettering not found".to_string()))?;

    let owner_user_id: Option<Uuid> =
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT user_id FROM letterings WHERE id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

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

    // Delete from Cloudflare R2
    let url_parts: Vec<&str> = lettering.image_url.split('/').collect();
    if let Some(filename) = url_parts.last() {
        let key = format!("letterings/{}", filename);
        if let Err(e) = state.storage.delete(&key).await {
            tracing::error!("Failed to delete R2 object {}: {}", key, e);
        }
        let _ = state
            .storage
            .delete(&format!("thumbnails/small/{}", filename))
            .await;
        let _ = state
            .storage
            .delete(&format!("thumbnails/medium/{}", filename))
            .await;
        let _ = state
            .storage
            .delete(&format!("thumbnails/large/{}", filename))
            .await;
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

/// Report an artifact. Increments report_count and appends the reason.
/// Items crossing the threshold (3 reports) are automatically hidden (REPORTED status).
pub async fn report_lettering(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ReportRequest>,
) -> Result<StatusCode, AppError> {
    let reason = body.reason.trim().to_string();
    if reason.is_empty() {
        return Err(AppError::BadRequest(
            "Report reason is required".to_string(),
        ));
    }

    let result = sqlx::query!(
        r#"UPDATE letterings
        SET report_count = report_count + 1,
            report_reasons = report_reasons || $2::jsonb,
            status = CASE WHEN report_count + 1 >= 3 THEN 'REPORTED' ELSE status END,
            updated_at = NOW()
        WHERE id = $1"#,
        id,
        serde_json::json!([reason]),
    )
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Lettering not found".to_string()));
    }

    tracing::info!(lettering_id = %id, "Lettering reported");
    Ok(StatusCode::OK)
}

pub async fn link_revisit(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<LinkRevisitRequest>,
) -> Result<StatusCode, AppError> {
    sqlx::query(
        r#"INSERT INTO location_revisits (id, original_lettering_id, revisit_lettering_id, notes)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (original_lettering_id, revisit_lettering_id) DO NOTHING"#,
    )
    .bind(Uuid::now_v7())
    .bind(id)
    .bind(body.revisit_lettering_id)
    .bind(body.notes)
    .execute(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    Ok(StatusCode::CREATED)
}

pub async fn get_revisits(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows = sqlx::query(
        r#"SELECT
                lr.id,
                lr.original_lettering_id,
                lr.revisit_lettering_id,
                lr.notes,
                lr.created_at,
                o.image_url as "original_image_url!",
                o.created_at as "original_created_at!",
                r.image_url as "revisit_image_url!",
                r.created_at as "revisit_created_at!"
           FROM location_revisits lr
           JOIN letterings o ON o.id = lr.original_lettering_id
           JOIN letterings r ON r.id = lr.revisit_lettering_id
           WHERE lr.original_lettering_id = $1 OR lr.revisit_lettering_id = $1
           ORDER BY lr.created_at DESC"#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    let revisits: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.get::<Uuid, _>("id"),
                "original_lettering_id": row.get::<Uuid, _>("original_lettering_id"),
                "revisit_lettering_id": row.get::<Uuid, _>("revisit_lettering_id"),
                "notes": row.get::<Option<String>, _>("notes"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "original": {
                    "image_url": row.get::<String, _>("original_image_url"),
                    "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("original_created_at")
                },
                "revisit": {
                    "image_url": row.get::<String, _>("revisit_image_url"),
                    "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("revisit_created_at")
                }
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "revisits": revisits })))
}
