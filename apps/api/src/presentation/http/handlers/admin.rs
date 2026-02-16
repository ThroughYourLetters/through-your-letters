use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use bcrypt::verify;
use chrono::{DateTime, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::{
    domain::lettering::repository::LetteringRepository,
    presentation::http::{errors::AppError, middleware::admin::AdminClaims, state::AppState},
};

async fn log_admin_action(
    state: &AppState,
    admin_sub: &str,
    action: &str,
    lettering_id: Option<Uuid>,
    metadata: serde_json::Value,
) {
    if let Err(e) = sqlx::query(
        "INSERT INTO admin_audit_logs (id, admin_sub, action, lettering_id, metadata) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(admin_sub)
    .bind(action)
    .bind(lettering_id)
    .bind(metadata)
    .execute(&state.db)
    .await
    {
        tracing::error!(
            "Failed to log admin action '{}' by '{}' for lettering {:?}: {}",
            action,
            admin_sub,
            lettering_id,
            e
        );
    }
}

async fn notify_lettering_owner(
    state: &AppState,
    lettering_id: Uuid,
    n_type: &str,
    title: &str,
    body: &str,
    metadata: serde_json::Value,
) {
    let owner_user_id: Option<Uuid> =
        match sqlx::query_scalar::<_, Option<Uuid>>("SELECT user_id FROM letterings WHERE id = $1")
            .bind(lettering_id)
            .fetch_one(&state.db)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    "Failed to fetch owner user_id for lettering {}: {}",
                    lettering_id,
                    e
                );
                None
            }
        };

    if let Some(user_id) = owner_user_id {
        if let Err(e) = sqlx::query(
            "INSERT INTO notifications (id, user_id, type, title, body, metadata) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::now_v7())
        .bind(user_id)
        .bind(n_type)
        .bind(title)
        .bind(body)
        .bind(metadata)
        .execute(&state.db)
        .await
        {
            tracing::error!(
                "Failed to create notification for user {} (lettering {}): {}",
                user_id,
                lettering_id,
                e
            );
        }
    }
}

// --- DTOs ---

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct ModerationQuery {
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_status() -> String {
    "ALL".to_string()
}
fn default_limit() -> i64 {
    50
}

#[derive(Debug, Deserialize)]
pub struct AuditLogsQuery {
    pub action: Option<String>,
    pub country_code: Option<String>,
    pub lettering_id: Option<Uuid>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Debug, Serialize)]
pub struct ModerationItem {
    pub id: Uuid,
    pub image_url: String,
    pub thumbnail_small: Option<String>,
    pub contributor_tag: String,
    pub pin_code: String,
    pub detected_text: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub likes_count: i32,
    pub comments_count: i32,
    pub report_count: i32,
    pub report_reasons: serde_json::Value,
    pub cultural_context: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ModerationQueueResponse {
    pub items: Vec<ModerationItem>,
    pub total: i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct AdminAuditLogItem {
    pub id: Uuid,
    pub admin_sub: String,
    pub action: String,
    pub lettering_id: Option<Uuid>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AdminAuditLogsResponse {
    pub items: Vec<AdminAuditLogItem>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub total_uploads: i64,
    pub pending_approvals: i64,
    pub approved: i64,
    pub rejected: i64,
    pub total_cities: i64,
    pub total_likes: i64,
    pub total_comments: i64,
}

#[derive(Debug, Deserialize)]
pub struct RejectRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BulkLetteringActionRequest {
    pub ids: Vec<Uuid>,
    pub action: String,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BulkActionFailure {
    pub id: Uuid,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct BulkActionResponse {
    pub requested: usize,
    pub processed: usize,
    pub failed: usize,
    pub failed_items: Vec<BulkActionFailure>,
}

// --- Handlers ---

pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    // Validate email
    if body.email != state.config.admin_email {
        return Err(AppError::Forbidden("Invalid credentials".to_string()));
    }

    // Verify password against bcrypt hash
    let valid = verify(&body.password, &state.config.admin_password_hash)
        .map_err(|_| AppError::Internal("Password verification failed".to_string()))?;

    if !valid {
        return Err(AppError::Forbidden("Invalid credentials".to_string()));
    }

    // Issue JWT valid for 24 hours
    let exp = (chrono::Utc::now() + chrono::Duration::hours(24)).timestamp() as usize;
    let claims = AdminClaims {
        sub: body.email.clone(),
        exp,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("Token generation failed: {}", e)))?;

    log_admin_action(
        &state,
        &body.email,
        "ADMIN_LOGIN",
        None,
        serde_json::json!({}),
    )
    .await;

    tracing::info!("Admin login successful");
    Ok(Json(LoginResponse { token }))
}

pub async fn get_moderation_queue(
    State(state): State<AppState>,
    Query(params): Query<ModerationQuery>,
) -> Result<Json<ModerationQueueResponse>, AppError> {
    let status_filter = params.status.to_uppercase();
    let safe_limit = params.limit.clamp(1, 200);
    let safe_offset = params.offset.max(0);

    let (items, total) = if status_filter == "ALL" {
        let items = sqlx::query_as!(
            ModerationItem,
            r#"SELECT id, image_url, thumbnail_small, contributor_tag, pin_code,
               detected_text, description, status, likes_count, comments_count,
               report_count, report_reasons, cultural_context, created_at
               FROM letterings
               ORDER BY created_at DESC
               LIMIT $1 OFFSET $2"#,
            safe_limit,
            safe_offset,
        )
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM letterings")
            .fetch_one(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        (items, total)
    } else {
        let items = sqlx::query_as!(
            ModerationItem,
            r#"SELECT id, image_url, thumbnail_small, contributor_tag, pin_code,
               detected_text, description, status, likes_count, comments_count,
               report_count, report_reasons, cultural_context, created_at
               FROM letterings
               WHERE status = $1
               ORDER BY created_at ASC
               LIMIT $2 OFFSET $3"#,
            status_filter,
            safe_limit,
            safe_offset,
        )
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM letterings WHERE status = $1",
        )
        .bind(status_filter)
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        (items, total)
    };

    Ok(Json(ModerationQueueResponse { items, total }))
}

pub async fn approve_lettering(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query(
        "UPDATE letterings
         SET status = 'APPROVED',
             moderation_reason = 'Approved by moderation',
             moderated_at = NOW(),
             moderated_by = $2,
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .bind(&claims.sub)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Lettering not found".to_string()));
    }

    log_admin_action(
        &state,
        &claims.sub,
        "APPROVE_LETTERING",
        Some(id),
        serde_json::json!({}),
    )
    .await;
    notify_lettering_owner(
        &state,
        id,
        "MODERATION_APPROVED",
        "Your upload was approved",
        "Your lettering contribution has been approved and is now publicly visible.",
        serde_json::json!({ "lettering_id": id }),
    )
    .await;

    tracing::info!(lettering_id = %id, "Lettering approved");
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reject_lettering(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
    Json(body): Json<RejectRequest>,
) -> Result<StatusCode, AppError> {
    let reason = body
        .reason
        .unwrap_or_else(|| "Rejected by admin".to_string());

    let result = sqlx::query(
        "UPDATE letterings
         SET status = 'REJECTED',
             moderation_reason = $2,
             moderated_at = NOW(),
             moderated_by = $3,
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .bind(reason.clone())
    .bind(&claims.sub)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Lettering not found".to_string()));
    }

    log_admin_action(
        &state,
        &claims.sub,
        "REJECT_LETTERING",
        Some(id),
        serde_json::json!({ "reason": reason.clone() }),
    )
    .await;
    notify_lettering_owner(
        &state,
        id,
        "MODERATION_REJECTED",
        "Your upload was rejected",
        "Your lettering contribution was rejected by moderation.",
        serde_json::json!({ "lettering_id": id, "reason": reason.clone() }),
    )
    .await;

    tracing::info!(lettering_id = %id, reason = %reason, "Lettering rejected");
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_any_lettering(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let lettering = state
        .lettering_repo
        .find_by_id(id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Lettering not found".to_string()))?;

    notify_lettering_owner(
        &state,
        id,
        "MODERATION_DELETED",
        "Your upload was deleted",
        "Your lettering contribution was removed by moderation.",
        serde_json::json!({ "lettering_id": id }),
    )
    .await;

    // Clean up R2 storage
    let url_parts: Vec<&str> = lettering.image_url.split('/').collect();
    if let Some(filename) = url_parts.last() {
        let _ = state
            .storage
            .delete(&format!("letterings/{}", filename))
            .await;
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

    state
        .lettering_repo
        .delete(id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    log_admin_action(
        &state,
        &claims.sub,
        "DELETE_LETTERING",
        Some(id),
        serde_json::json!({}),
    )
    .await;

    tracing::info!(lettering_id = %id, "Lettering deleted by admin");
    Ok(StatusCode::NO_CONTENT)
}

/// "Keep & Clear": Resets report_count to 0, clears reasons, restores status to APPROVED
pub async fn clear_reports(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query(
        r#"UPDATE letterings
        SET report_count = 0,
            report_reasons = '[]'::jsonb,
            status = 'APPROVED',
            moderation_reason = 'Reports cleared after moderator review',
            moderated_at = NOW(),
            moderated_by = $2,
            updated_at = NOW()
        WHERE id = $1"#,
    )
    .bind(id)
    .bind(&claims.sub)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Lettering not found".to_string()));
    }

    log_admin_action(
        &state,
        &claims.sub,
        "CLEAR_REPORTS",
        Some(id),
        serde_json::json!({}),
    )
    .await;
    notify_lettering_owner(
        &state,
        id,
        "REPORTS_CLEARED",
        "Reports cleared on your upload",
        "Moderator reviewed and cleared reports on your lettering contribution.",
        serde_json::json!({ "lettering_id": id }),
    )
    .await;

    tracing::info!(lettering_id = %id, "Reports cleared by admin");
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_stats(State(state): State<AppState>) -> Result<Json<StatsResponse>, AppError> {
    let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM letterings")
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let pending = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM letterings WHERE status = 'PENDING'")
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let approved = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM letterings WHERE status = 'APPROVED'")
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let rejected = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM letterings WHERE status = 'REJECTED'")
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let cities = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cities")
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let likes = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM likes")
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let comments = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM comments")
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(StatsResponse {
        total_uploads: total,
        pending_approvals: pending,
        approved,
        rejected,
        total_cities: cities,
        total_likes: likes,
        total_comments: comments,
    }))
}

pub async fn list_audit_logs(
    State(state): State<AppState>,
    Query(params): Query<AuditLogsQuery>,
) -> Result<Json<AdminAuditLogsResponse>, AppError> {
    let safe_limit = params.limit.clamp(1, 200);
    let safe_offset = params.offset.max(0);
    let action = params
        .action
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_uppercase());
    let country_code = params
        .country_code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_uppercase());

    let mut data_qb = QueryBuilder::<Postgres>::new(
        "SELECT id, admin_sub, action, lettering_id, metadata, created_at
         FROM admin_audit_logs
         WHERE 1=1",
    );
    if let Some(action) = &action {
        data_qb.push(" AND action = ").push_bind(action);
    }
    if let Some(lettering_id) = params.lettering_id {
        data_qb.push(" AND lettering_id = ").push_bind(lettering_id);
    }
    if let Some(country_code) = &country_code {
        data_qb
            .push(" AND UPPER(COALESCE(metadata->>'country_code', '')) = ")
            .push_bind(country_code);
    }
    data_qb
        .push(" ORDER BY created_at DESC LIMIT ")
        .push_bind(safe_limit)
        .push(" OFFSET ")
        .push_bind(safe_offset);

    let items: Vec<AdminAuditLogItem> = data_qb
        .build_query_as()
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut count_qb =
        QueryBuilder::<Postgres>::new("SELECT COUNT(*)::bigint FROM admin_audit_logs WHERE 1=1");
    if let Some(action) = &action {
        count_qb.push(" AND action = ").push_bind(action);
    }
    if let Some(lettering_id) = params.lettering_id {
        count_qb
            .push(" AND lettering_id = ")
            .push_bind(lettering_id);
    }
    if let Some(country_code) = &country_code {
        count_qb
            .push(" AND UPPER(COALESCE(metadata->>'country_code', '')) = ")
            .push_bind(country_code);
    }
    let total: i64 = count_qb
        .build_query_scalar()
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(AdminAuditLogsResponse {
        items,
        total,
        limit: safe_limit,
        offset: safe_offset,
    }))
}

pub async fn bulk_lettering_action(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Json(body): Json<BulkLetteringActionRequest>,
) -> Result<Json<BulkActionResponse>, AppError> {
    let action = body.action.trim().to_lowercase();
    if !["approve", "reject", "delete", "keep"].contains(&action.as_str()) {
        return Err(AppError::BadRequest(
            "action must be one of approve, reject, delete, keep".to_string(),
        ));
    }
    if body.ids.is_empty() {
        return Err(AppError::BadRequest("ids cannot be empty".to_string()));
    }
    if body.ids.len() > 200 {
        return Err(AppError::BadRequest(
            "bulk actions are limited to 200 items".to_string(),
        ));
    }

    let mut failed_items = Vec::new();
    let mut processed = 0usize;
    let reason = body
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Bulk moderation action");

    for id in body.ids.iter().copied() {
        let result: Result<(), AppError> = match action.as_str() {
            "approve" => {
                let result = sqlx::query(
                    "UPDATE letterings
                     SET status = 'APPROVED',
                         moderation_reason = 'Approved by bulk moderation',
                         moderated_at = NOW(),
                         moderated_by = $2,
                         updated_at = NOW()
                     WHERE id = $1",
                )
                .bind(id)
                .bind(&claims.sub)
                .execute(&state.db)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
                if result.rows_affected() == 0 {
                    Err(AppError::NotFound("Lettering not found".to_string()))
                } else {
                    log_admin_action(
                        &state,
                        &claims.sub,
                        "BULK_APPROVE_LETTERING",
                        Some(id),
                        serde_json::json!({}),
                    )
                    .await;
                    notify_lettering_owner(
                        &state,
                        id,
                        "MODERATION_APPROVED",
                        "Your upload was approved",
                        "Your lettering contribution has been approved and is now publicly visible.",
                        serde_json::json!({ "lettering_id": id }),
                    )
                    .await;
                    Ok(())
                }
            }
            "reject" => {
                let result = sqlx::query(
                    "UPDATE letterings
                     SET status = 'REJECTED',
                         moderation_reason = $2,
                         moderated_at = NOW(),
                         moderated_by = $3,
                         updated_at = NOW()
                     WHERE id = $1",
                )
                .bind(id)
                .bind(reason)
                .bind(&claims.sub)
                .execute(&state.db)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
                if result.rows_affected() == 0 {
                    Err(AppError::NotFound("Lettering not found".to_string()))
                } else {
                    log_admin_action(
                        &state,
                        &claims.sub,
                        "BULK_REJECT_LETTERING",
                        Some(id),
                        serde_json::json!({ "reason": reason }),
                    )
                    .await;
                    notify_lettering_owner(
                        &state,
                        id,
                        "MODERATION_REJECTED",
                        "Your upload was rejected",
                        "Your lettering contribution was rejected by moderation.",
                        serde_json::json!({ "lettering_id": id, "reason": reason }),
                    )
                    .await;
                    Ok(())
                }
            }
            "keep" => {
                let result = sqlx::query(
                    r#"UPDATE letterings
                       SET report_count = 0,
                           report_reasons = '[]'::jsonb,
                           status = 'APPROVED',
                           moderation_reason = 'Reports cleared after moderator review',
                           moderated_at = NOW(),
                           moderated_by = $2,
                           updated_at = NOW()
                       WHERE id = $1"#,
                )
                .bind(id)
                .bind(&claims.sub)
                .execute(&state.db)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
                if result.rows_affected() == 0 {
                    Err(AppError::NotFound("Lettering not found".to_string()))
                } else {
                    log_admin_action(
                        &state,
                        &claims.sub,
                        "BULK_CLEAR_REPORTS",
                        Some(id),
                        serde_json::json!({}),
                    )
                    .await;
                    notify_lettering_owner(
                        &state,
                        id,
                        "REPORTS_CLEARED",
                        "Reports cleared on your upload",
                        "Moderator reviewed and cleared reports on your lettering contribution.",
                        serde_json::json!({ "lettering_id": id }),
                    )
                    .await;
                    Ok(())
                }
            }
            _ => {
                let lettering = state
                    .lettering_repo
                    .find_by_id(id)
                    .await
                    .map_err(|e| AppError::Internal(e.to_string()))?
                    .ok_or_else(|| AppError::NotFound("Lettering not found".to_string()))?;

                notify_lettering_owner(
                    &state,
                    id,
                    "MODERATION_DELETED",
                    "Your upload was deleted",
                    "Your lettering contribution was removed by moderation.",
                    serde_json::json!({ "lettering_id": id }),
                )
                .await;

                let url_parts: Vec<&str> = lettering.image_url.split('/').collect();
                if let Some(filename) = url_parts.last() {
                    let _ = state
                        .storage
                        .delete(&format!("letterings/{}", filename))
                        .await;
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

                state
                    .lettering_repo
                    .delete(id)
                    .await
                    .map_err(|e| AppError::Internal(e.to_string()))?;

                log_admin_action(
                    &state,
                    &claims.sub,
                    "BULK_DELETE_LETTERING",
                    Some(id),
                    serde_json::json!({}),
                )
                .await;
                Ok(())
            }
        };

        match result {
            Ok(()) => processed += 1,
            Err(err) => failed_items.push(BulkActionFailure {
                id,
                error: match err {
                    AppError::NotFound(msg) => msg,
                    AppError::Forbidden(msg) => msg,
                    AppError::BadRequest(msg) => msg,
                    AppError::ValidationError(msg) => msg,
                    AppError::RateLimited => "Rate limited".to_string(),
                    AppError::Database(msg) => msg,
                    AppError::Storage(msg) => msg,
                    AppError::MlProcessing(msg) => msg,
                    AppError::Queue(msg) => msg,
                    AppError::ExternalService(msg) => msg,
                    AppError::Internal(msg) => msg,
                },
            }),
        }
    }

    Ok(Json(BulkActionResponse {
        requested: body.ids.len(),
        processed,
        failed: failed_items.len(),
        failed_items,
    }))
}
