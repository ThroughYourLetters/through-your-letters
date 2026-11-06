use crate::presentation::http::pagination::{Page, TimePosition, continuation, decode_cursor};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};
use ts_rs::TS;
use uuid::Uuid;

use crate::{
    domain::lettering::value_objects::PinCode,
    presentation::http::{
        bounded_json::BoundedJson, errors::AppError, middleware::user::decode_required_user_claims,
        state::AppState,
    },
};

#[derive(Debug, Deserialize)]
pub struct MyUploadsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub status: Option<String>,
    pub after: Option<String>,
}

fn default_limit() -> i64 {
    20
}

#[derive(Debug, Serialize, FromRow, TS)]
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

#[derive(Debug, Serialize, TS)]
pub struct MyUploadsResponse {
    pub items: Vec<MyUploadItem>,
    #[ts(type = "number")]
    pub total: i64,
    #[ts(type = "number")]
    pub limit: i64,
    #[ts(type = "number")]
    pub offset: i64,
    pub next_cursor: Option<String>,
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

#[derive(Debug, Serialize, FromRow, TS)]
pub struct MyUploadStatusHistoryItem {
    pub id: Uuid,
    pub from_status: Option<String>,
    pub to_status: String,
    pub reason: Option<String>,
    pub actor_type: String,
    pub actor_sub: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow, TS)]
pub struct MyUploadMetadataHistoryItem {
    pub id: Uuid,
    pub field_name: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, TS)]
pub struct MyUploadTimelineResponse {
    pub status_history: Vec<MyUploadStatusHistoryItem>,
    pub metadata_history: Vec<MyUploadMetadataHistoryItem>,
    pub next_status_cursor: Option<String>,
    pub next_metadata_cursor: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimelineSection {
    #[default]
    All,
    Status,
    Metadata,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineQuery {
    pub limit: Option<i64>,
    pub after: Option<String>,
    #[serde(default)]
    pub section: TimelineSection,
}

#[derive(Debug, Serialize, FromRow, TS)]
pub struct NotificationItem {
    pub id: Uuid,
    pub r#type: String,
    pub title: String,
    pub body: Option<String>,
    #[ts(type = "Record<string, unknown>")]
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
    pub after: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct NotificationsResponse {
    pub items: Vec<NotificationItem>,
    #[ts(type = "number")]
    pub total: i64,
    #[ts(type = "number")]
    pub unread: i64,
    #[ts(type = "number")]
    pub limit: i64,
    #[ts(type = "number")]
    pub offset: i64,
    pub next_cursor: Option<String>,
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
            if !(2..=30).contains(&len) {
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
    let page = Page::new(
        params.limit,
        params.offset,
        100,
        &state.config.limits.pagination,
    )?;
    if params.after.is_some() && page.offset() != 0 {
        return Err(AppError::BadRequest(
            "Use a cursor or an offset, not both".into(),
        ));
    }
    let status = params.status.as_deref().map(str::to_uppercase);
    if status
        .as_deref()
        .is_some_and(|status| !["PENDING", "APPROVED", "REJECTED", "REPORTED"].contains(&status))
    {
        return Err(AppError::BadRequest("Invalid status filter".into()));
    }
    let scope = format!("uploads:{user_id}:{}", status.as_deref().unwrap_or("all"));
    let after: Option<TimePosition> = decode_cursor(
        params.after.as_deref(),
        &scope,
        &state.config.limits.pagination,
    )?;
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT id, image_url, thumbnail_small, pin_code, contributor_tag, detected_text, description,
         status, likes_count, comments_count, report_count, moderation_reason, moderated_at,
         moderated_by, created_at, updated_at FROM letterings WHERE user_id = ",
    );
    query.push_bind(user_id);
    if let Some(status) = &status {
        query.push(" AND status = ").push_bind(status);
    }
    if let Some(after) = after {
        query
            .push(" AND (created_at,id) < (")
            .push_bind(after.created_at)
            .push(",")
            .push_bind(after.id)
            .push(")");
    }
    query
        .push(" ORDER BY created_at DESC,id DESC LIMIT ")
        .push_bind(page.limit() + 1)
        .push(" OFFSET ")
        .push_bind(page.offset());
    let mut items: Vec<MyUploadItem> = query.build_query_as().fetch_all(&state.db).await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM letterings WHERE user_id=$1 AND ($2::text IS NULL OR status=$2)",
    )
    .bind(user_id)
    .bind(status)
    .fetch_one(&state.db)
    .await?;
    let next_cursor = continuation(&mut items, page, &scope, |row| TimePosition {
        created_at: row.created_at,
        id: row.id,
    })?;
    Ok(Json(MyUploadsResponse {
        items,
        total,
        limit: page.limit(),
        offset: page.offset(),
        next_cursor,
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

    let pin_input = body
        .pin_code
        .map(PinCode::new)
        .transpose()
        .map_err(|error| AppError::BadRequest(format!("Invalid postal code: {error}")))?
        .map(|pin| pin.value);
    let description_input = normalize_optional_description(body.description)?;
    let contributor_input = normalize_optional_contributor_tag(body.contributor_tag)?;

    let mut tx = state.db.begin().await?;
    // Serialize edits with reviewed location corrections.
    let existing = sqlx::query_as::<_, MyUploadEditableRow>(
        "SELECT id, description, contributor_tag, pin_code
         FROM letterings
         WHERE id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Forbidden("You can only update your own uploads".to_string()))?;

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
    let resolved_contributor =
        contributor_input.unwrap_or_else(|| existing.contributor_tag.clone());
    let resolved_pin = pin_input.unwrap_or_else(|| existing.pin_code.clone());

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
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        tracing::info!(user_id = %user_id, lettering_id = %existing.id, "No metadata changes detected");
        return Ok(Json(current));
    }

    // Postal edits invalidate the previous location.
    let updated = sqlx::query_as::<_, MyUploadItem>(
        "UPDATE letterings
         SET description = $1,
             contributor_tag = $2,
             pin_code = $3,
             location = CASE WHEN $6 THEN NULL ELSE location END,
             location_source = CASE WHEN $6 THEN 'unresolved' ELSE location_source END,
             location_error = CASE WHEN $6 THEN 'postal_code_changed' ELSE location_error END,
             updated_at = clock_timestamp()
         WHERE id = $4 AND user_id = $5
         RETURNING id, image_url, thumbnail_small, pin_code, contributor_tag, detected_text, description, status, likes_count, comments_count, report_count, moderation_reason, moderated_at, moderated_by, created_at, updated_at",
    )
    .bind(&resolved_description)
    .bind(&resolved_contributor)
    .bind(&resolved_pin)
    .bind(existing.id)
    .bind(user_id)
    .bind(changed_pin)
    .fetch_one(&mut *tx)
    .await?;

    if changed_description {
        sqlx::query(
            "INSERT INTO lettering_metadata_history (id, lettering_id, edited_by_user_id, field_name, old_value, new_value, created_at)
             VALUES ($1, $2, $3, 'description', $4, $5, clock_timestamp())",
        )
        .bind(Uuid::now_v7())
        .bind(existing.id)
        .bind(user_id)
        .bind(existing.description)
        .bind(updated.description.clone())
        .execute(&mut *tx)
        .await?;
    }

    if changed_contributor {
        sqlx::query(
            "INSERT INTO lettering_metadata_history (id, lettering_id, edited_by_user_id, field_name, old_value, new_value, created_at)
             VALUES ($1, $2, $3, 'contributor_tag', $4, $5, clock_timestamp())",
        )
        .bind(Uuid::now_v7())
        .bind(existing.id)
        .bind(user_id)
        .bind(existing.contributor_tag)
        .bind(updated.contributor_tag.clone())
        .execute(&mut *tx)
        .await?;
    }

    if changed_pin {
        sqlx::query(
            "INSERT INTO lettering_metadata_history (id, lettering_id, edited_by_user_id, field_name, old_value, new_value, created_at)
             VALUES ($1, $2, $3, 'pin_code', $4, $5, clock_timestamp())",
        )
        .bind(Uuid::now_v7())
        .bind(existing.id)
        .bind(user_id)
        .bind(existing.pin_code)
        .bind(updated.pin_code.clone())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

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
    Query(query): Query<TimelineQuery>,
) -> Result<Json<MyUploadTimelineResponse>, AppError> {
    let user_id = parse_user_id(&headers, &state)?;
    let owned: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM letterings WHERE id=$1 AND user_id=$2)")
            .bind(id)
            .bind(user_id)
            .fetch_one(&state.db)
            .await?;
    if !owned {
        return Err(AppError::Forbidden(
            "You can only view your own upload timeline".into(),
        ));
    }
    let page = Page::new(
        query.limit.unwrap_or(100),
        0,
        100,
        &state.config.limits.pagination,
    )?;
    if matches!(query.section, TimelineSection::All) && query.after.is_some() {
        return Err(AppError::BadRequest(
            "Choose status or metadata when continuing a timeline".into(),
        ));
    }
    let status_scope = format!("timeline:{user_id}:{id}:status");
    let metadata_scope = format!("timeline:{user_id}:{id}:metadata");
    let scope = if matches!(query.section, TimelineSection::Metadata) {
        &metadata_scope
    } else {
        &status_scope
    };
    let after: Option<TimePosition> = decode_cursor(
        query.after.as_deref(),
        scope,
        &state.config.limits.pagination,
    )?;
    let mut status_history = Vec::new();
    let mut metadata_history = Vec::new();
    if !matches!(query.section, TimelineSection::Metadata) {
        let mut sql = QueryBuilder::<Postgres>::new(
            "SELECT h.id,from_status,to_status,reason,actor_type,actor_sub,h.created_at FROM lettering_status_history h JOIN letterings l ON l.id=h.lettering_id WHERE l.id=",
        );
        sql.push_bind(id).push(" AND l.user_id=").push_bind(user_id);
        if let Some(after) = after {
            sql.push(" AND (h.created_at,h.id) < (")
                .push_bind(after.created_at)
                .push(",")
                .push_bind(after.id)
                .push(")");
        }
        sql.push(" ORDER BY h.created_at DESC,h.id DESC LIMIT ")
            .push_bind(page.limit() + 1);
        status_history = sql
            .build_query_as::<MyUploadStatusHistoryItem>()
            .fetch_all(&state.db)
            .await?;
    }
    if !matches!(query.section, TimelineSection::Status) {
        let mut sql = QueryBuilder::<Postgres>::new(
            "SELECT h.id,field_name,old_value,new_value,h.created_at FROM lettering_metadata_history h JOIN letterings l ON l.id=h.lettering_id WHERE l.id=",
        );
        sql.push_bind(id).push(" AND l.user_id=").push_bind(user_id);
        if let Some(after) = after {
            sql.push(" AND (h.created_at,h.id) < (")
                .push_bind(after.created_at)
                .push(",")
                .push_bind(after.id)
                .push(")");
        }
        sql.push(" ORDER BY h.created_at DESC,h.id DESC LIMIT ")
            .push_bind(page.limit() + 1);
        metadata_history = sql
            .build_query_as::<MyUploadMetadataHistoryItem>()
            .fetch_all(&state.db)
            .await?;
    }
    let next_status_cursor = continuation(&mut status_history, page, &status_scope, |row| {
        TimePosition {
            created_at: row.created_at,
            id: row.id,
        }
    })?;
    let next_metadata_cursor = continuation(&mut metadata_history, page, &metadata_scope, |row| {
        TimePosition {
            created_at: row.created_at,
            id: row.id,
        }
    })?;
    Ok(Json(MyUploadTimelineResponse {
        status_history,
        metadata_history,
        next_status_cursor,
        next_metadata_cursor,
    }))
}

pub async fn list_notifications(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<NotificationsQuery>,
) -> Result<Json<NotificationsResponse>, AppError> {
    let user_id = parse_user_id(&headers, &state)?;
    let page = Page::new(
        params.limit,
        params.offset,
        100,
        &state.config.limits.pagination,
    )?;
    if params.after.is_some() && page.offset() != 0 {
        return Err(AppError::BadRequest(
            "Use a cursor or an offset, not both".into(),
        ));
    }
    let scope = format!("notifications:{user_id}");
    let after: Option<TimePosition> = decode_cursor(
        params.after.as_deref(),
        &scope,
        &state.config.limits.pagination,
    )?;
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT id,type,title,body,metadata,is_read,created_at FROM notifications WHERE user_id = ",
    );
    query.push_bind(user_id);
    if let Some(after) = after {
        query
            .push(" AND (created_at,id) < (")
            .push_bind(after.created_at)
            .push(",")
            .push_bind(after.id)
            .push(")");
    }
    query
        .push(" ORDER BY created_at DESC,id DESC LIMIT ")
        .push_bind(page.limit() + 1)
        .push(" OFFSET ")
        .push_bind(page.offset());
    let mut items: Vec<NotificationItem> = query.build_query_as().fetch_all(&state.db).await?;
    let (total, unread): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),COUNT(*) FILTER(WHERE NOT is_read) FROM notifications WHERE user_id=$1",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;
    let next_cursor = continuation(&mut items, page, &scope, |row| TimePosition {
        created_at: row.created_at,
        id: row.id,
    })?;
    Ok(Json(NotificationsResponse {
        items,
        total,
        unread,
        limit: page.limit(),
        offset: page.offset(),
        next_cursor,
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
            .map_err(AppError::internal)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Notification not found".to_string()));
    }

    Ok(StatusCode::OK)
}

// ── Account deletion ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DeleteAccountRequest {
    /// Must provide current password to confirm intent
    pub password: String,
}

/// Remove account identity while retaining anonymous public contributions.
pub async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    BoundedJson(body): BoundedJson<DeleteAccountRequest>,
) -> Result<StatusCode, AppError> {
    let user_id = parse_user_id(&headers, &state)?;

    #[derive(sqlx::FromRow)]
    struct PasswordRow {
        password_hash: String,
    }

    let row = sqlx::query_as::<_, PasswordRow>(
        "SELECT password_hash FROM users WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?
    .ok_or_else(|| AppError::Forbidden("User not found".to_string()))?;

    let valid = state
        .passwords
        .verify(body.password, row.password_hash.clone())
        .await?;
    if !valid {
        return Err(AppError::Forbidden(
            "Incorrect password — account deletion cancelled".to_string(),
        ));
    }

    let mut tx = state.db.begin().await.map_err(AppError::database)?;
    let current_hash: Option<String> = sqlx::query_scalar(
        "SELECT password_hash FROM users WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::database)?;
    if current_hash.as_deref() != Some(row.password_hash.as_str()) {
        return Err(AppError::Unauthorized);
    }
    for statement in [
        "UPDATE letterings SET user_id = NULL, contributor_tag = 'Anonymous', uploaded_by_ip = NULL WHERE user_id = $1",
        "UPDATE comments SET user_id = NULL, content = '[deleted]', user_ip = NULL WHERE user_id = $1",
        "DELETE FROM collections WHERE owner_user_id = $1 AND NOT is_public",
        "UPDATE collections SET owner_user_id = NULL, creator_tag = 'Anonymous' WHERE owner_user_id = $1",
        "DELETE FROM upload_requests WHERE owner_id = $1",
        "DELETE FROM password_reset_tokens WHERE user_id = $1",
        "DELETE FROM notifications WHERE user_id = $1",
        "UPDATE users SET deleted_at = NOW(), session_version = session_version + 1, email = 'deleted_' || id::text, display_name = NULL, password_hash = '', email_verification_token_hash = NULL, email_verification_expires_at = NULL, updated_at = NOW() WHERE id = $1",
    ] {
        sqlx::query(statement)
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
    }
    tx.commit().await.map_err(AppError::database)?;
    state.cache.invalidate().await;

    tracing::info!(user_id = %user_id, "Account deleted (soft delete)");
    Ok(StatusCode::NO_CONTENT)
}
