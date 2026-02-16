use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::presentation::http::{
    errors::AppError, middleware::user::decode_required_user_claims, state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct MyUploadsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub status: Option<String>,
}

fn default_limit() -> i64 {
    20
}

fn safe_limit_offset(limit: i64, offset: i64) -> (i64, i64) {
    (limit.clamp(1, 100), offset.max(0))
}

#[derive(Debug, Serialize, FromRow)]
pub struct MyUploadItem {
    pub id: Uuid,
    pub image_url: String,
    pub thumbnail_small: String,
    pub pin_code: String,
    pub contributor_tag: String,
    pub detected_text: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub likes_count: i32,
    pub comments_count: i32,
    pub report_count: i32,
    pub moderation_reason: Option<String>,
    pub moderated_at: Option<DateTime<Utc>>,
    pub moderated_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct MyUploadsResponse {
    pub items: Vec<MyUploadItem>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMyUploadRequest {
    pub description: Option<String>,
    pub contributor_tag: Option<String>,
    pub pin_code: Option<String>,
}

#[derive(Debug, FromRow)]
struct MyUploadEditableRow {
    id: Uuid,
    description: Option<String>,
    contributor_tag: String,
    pin_code: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct MyUploadStatusHistoryItem {
    pub id: Uuid,
    pub from_status: Option<String>,
    pub to_status: String,
    pub reason: Option<String>,
    pub actor_type: String,
    pub actor_sub: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct MyUploadMetadataHistoryItem {
    pub id: Uuid,
    pub field_name: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct MyUploadTimelineResponse {
    pub status_history: Vec<MyUploadStatusHistoryItem>,
    pub metadata_history: Vec<MyUploadMetadataHistoryItem>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct NotificationItem {
    pub id: Uuid,
    pub r#type: String,
    pub title: String,
    pub body: Option<String>,
    pub metadata: serde_json::Value,
    pub is_read: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct NotificationsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Debug, Serialize)]
pub struct NotificationsResponse {
    pub items: Vec<NotificationItem>,
    pub total: i64,
    pub unread: i64,
    pub limit: i64,
    pub offset: i64,
}

fn parse_user_id(headers: &HeaderMap, state: &AppState) -> Result<Uuid, AppError> {
    let claims = decode_required_user_claims(headers, &state.config.jwt_secret)?;
    Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Forbidden("Invalid token subject".to_string()))
}

fn normalize_optional_description(value: Option<String>) -> Result<Option<String>, AppError> {
    match value {
        None => Ok(None),
        Some(v) => {
            let trimmed = v.trim().to_string();
            if trimmed.is_empty() {
                return Ok(Some(String::new()));
            }
            if trimmed.chars().count() > 1200 {
                return Err(AppError::BadRequest(
                    "description must be 1200 characters or less".to_string(),
                ));
            }
            Ok(Some(trimmed))
        }
    }
}

fn normalize_optional_contributor_tag(value: Option<String>) -> Result<Option<String>, AppError> {
    match value {
        None => Ok(None),
        Some(v) => {
            let trimmed = v.trim().to_string();
            let len = trimmed.chars().count();
            if len < 2 || len > 30 {
                return Err(AppError::BadRequest(
                    "contributor_tag must be between 2 and 30 characters".to_string(),
                ));
            }
            let valid_chars = trimmed
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '_' || c == '-' || c == '.');
            if !valid_chars {
                return Err(AppError::BadRequest(
                    "contributor_tag contains unsupported characters".to_string(),
                ));
            }
            Ok(Some(trimmed))
        }
    }
}

pub async fn list_my_letterings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<MyUploadsQuery>,
) -> Result<Json<MyUploadsResponse>, AppError> {
    let user_id = parse_user_id(&headers, &state)?;
    let (safe_limit, safe_offset) = safe_limit_offset(params.limit, params.offset);
    let status = params.status.as_ref().map(|s| s.to_uppercase());

    let (items, total) = if let Some(ref status_filter) = status {
        let allowed = ["PENDING", "APPROVED", "REJECTED", "REPORTED"];
        if !allowed.contains(&status_filter.as_str()) {
            return Err(AppError::BadRequest("Invalid status filter".to_string()));
        }

        let items = sqlx::query_as::<_, MyUploadItem>(
            r#"SELECT id, image_url, thumbnail_small, pin_code, contributor_tag, detected_text, description,
                      status, likes_count, comments_count, report_count, moderation_reason, moderated_at,
                      moderated_by, created_at, updated_at
               FROM letterings
               WHERE user_id = $1 AND status = $2
               ORDER BY created_at DESC
               LIMIT $3 OFFSET $4"#,
        )
        .bind(user_id)
        .bind(status_filter)
        .bind(safe_limit)
        .bind(safe_offset)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM letterings WHERE user_id = $1 AND status = $2",
        )
        .bind(user_id)
        .bind(status_filter)
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        (items, total)
    } else {
        let items = sqlx::query_as::<_, MyUploadItem>(
            r#"SELECT id, image_url, thumbnail_small, pin_code, contributor_tag, detected_text, description,
                      status, likes_count, comments_count, report_count, moderation_reason, moderated_at,
                      moderated_by, created_at, updated_at
               FROM letterings
               WHERE user_id = $1
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(user_id)
        .bind(safe_limit)
        .bind(safe_offset)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let total =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM letterings WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(&state.db)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

        (items, total)
    };

    Ok(Json(MyUploadsResponse {
        items,
        total,
        limit: safe_limit,
        offset: safe_offset,
    }))
}

pub async fn update_my_lettering(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateMyUploadRequest>,
) -> Result<Json<MyUploadItem>, AppError> {
    let user_id = parse_user_id(&headers, &state)?;

    if body.description.is_none() && body.contributor_tag.is_none() && body.pin_code.is_none() {
        return Err(AppError::BadRequest("No updates provided".to_string()));
    }

    if let Some(pin) = body.pin_code.as_deref() {
        if pin.len() != 6 || !pin.chars().all(|c| c.is_ascii_digit()) {
            return Err(AppError::BadRequest(
                "pin_code must be 6 digits".to_string(),
            ));
        }
    }

    let existing = sqlx::query_as::<_, MyUploadEditableRow>(
        "SELECT id, description, contributor_tag, pin_code
         FROM letterings
         WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
    .ok_or_else(|| AppError::Forbidden("You can only update your own uploads".to_string()))?;

    let description_input = normalize_optional_description(body.description)?;
    let contributor_input = normalize_optional_contributor_tag(body.contributor_tag)?;

    let resolved_description = match description_input {
        Some(v) => {
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        }
        None => existing.description.clone(),
    };
    let resolved_contributor = contributor_input.unwrap_or(existing.contributor_tag.clone());
    let resolved_pin = body.pin_code.unwrap_or(existing.pin_code.clone());

    let changed_description = existing.description != resolved_description;
    let changed_contributor = existing.contributor_tag != resolved_contributor;
    let changed_pin = existing.pin_code != resolved_pin;

    if !changed_description && !changed_contributor && !changed_pin {
        let current = sqlx::query_as::<_, MyUploadItem>(
            "SELECT id, image_url, thumbnail_small, pin_code, contributor_tag, detected_text, description, status, likes_count, comments_count, report_count, moderation_reason, moderated_at, moderated_by, created_at, updated_at
             FROM letterings
             WHERE id = $1 AND user_id = $2",
        )
        .bind(existing.id)
        .bind(user_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
        tracing::info!(user_id = %user_id, lettering_id = %existing.id, "No metadata changes detected");
        return Ok(Json(current));
    }

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let updated = sqlx::query_as::<_, MyUploadItem>(
        "UPDATE letterings
         SET description = $1,
             contributor_tag = $2,
             pin_code = $3,
             updated_at = NOW()
         WHERE id = $4 AND user_id = $5
         RETURNING id, image_url, thumbnail_small, pin_code, contributor_tag, detected_text, description, status, likes_count, comments_count, report_count, moderation_reason, moderated_at, moderated_by, created_at, updated_at",
    )
    .bind(&resolved_description)
    .bind(&resolved_contributor)
    .bind(&resolved_pin)
    .bind(existing.id)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    if changed_description {
        sqlx::query(
            "INSERT INTO lettering_metadata_history (id, lettering_id, edited_by_user_id, field_name, old_value, new_value)
             VALUES ($1, $2, $3, 'description', $4, $5)",
        )
        .bind(Uuid::now_v7())
        .bind(existing.id)
        .bind(user_id)
        .bind(existing.description)
        .bind(updated.description.clone())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    }

    if changed_contributor {
        sqlx::query(
            "INSERT INTO lettering_metadata_history (id, lettering_id, edited_by_user_id, field_name, old_value, new_value)
             VALUES ($1, $2, $3, 'contributor_tag', $4, $5)",
        )
        .bind(Uuid::now_v7())
        .bind(existing.id)
        .bind(user_id)
        .bind(existing.contributor_tag)
        .bind(updated.contributor_tag.clone())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    }

    if changed_pin {
        sqlx::query(
            "INSERT INTO lettering_metadata_history (id, lettering_id, edited_by_user_id, field_name, old_value, new_value)
             VALUES ($1, $2, $3, 'pin_code', $4, $5)",
        )
        .bind(Uuid::now_v7())
        .bind(existing.id)
        .bind(user_id)
        .bind(existing.pin_code)
        .bind(updated.pin_code.clone())
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    tracing::info!(
        user_id = %user_id,
        lettering_id = %existing.id,
        changed_description,
        changed_contributor,
        changed_pin,
        "User updated upload metadata"
    );

    Ok(Json(updated))
}

pub async fn get_my_lettering_timeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<MyUploadTimelineResponse>, AppError> {
    let user_id = parse_user_id(&headers, &state)?;

    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM letterings WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    if exists == 0 {
        tracing::warn!(
            user_id = %user_id,
            lettering_id = %id,
            "Timeline access denied for non-owned upload"
        );
        return Err(AppError::Forbidden(
            "You can only view your own upload timeline".to_string(),
        ));
    }

    let status_history = sqlx::query_as::<_, MyUploadStatusHistoryItem>(
        "SELECT id, from_status, to_status, reason, actor_type, actor_sub, created_at
         FROM lettering_status_history
         WHERE lettering_id = $1
         ORDER BY created_at DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let metadata_history = sqlx::query_as::<_, MyUploadMetadataHistoryItem>(
        "SELECT id, field_name, old_value, new_value, created_at
         FROM lettering_metadata_history
         WHERE lettering_id = $1
         ORDER BY created_at DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(MyUploadTimelineResponse {
        status_history,
        metadata_history,
    }))
}

pub async fn list_notifications(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<NotificationsQuery>,
) -> Result<Json<NotificationsResponse>, AppError> {
    let user_id = parse_user_id(&headers, &state)?;
    let (safe_limit, safe_offset) = safe_limit_offset(params.limit, params.offset);

    let items = sqlx::query_as::<_, NotificationItem>(
        "SELECT id, type, title, body, metadata, is_read, created_at FROM notifications WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(safe_limit)
    .bind(safe_offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let total =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM notifications WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

    let unread = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND is_read = false",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(NotificationsResponse {
        items,
        total,
        unread,
        limit: safe_limit,
        offset: safe_offset,
    }))
}

pub async fn mark_notification_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let user_id = parse_user_id(&headers, &state)?;

    let result =
        sqlx::query("UPDATE notifications SET is_read = true WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Notification not found".to_string()));
    }

    Ok(StatusCode::OK)
}
