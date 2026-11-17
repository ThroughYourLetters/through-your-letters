use crate::presentation::http::bounded_json::LimitedJson;
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};
use ts_rs::TS;
use uuid::Uuid;

use crate::presentation::http::{
    bounded_json::BoundedJson,
    errors::AppError,
    middleware::{admin::AdminClaims, client_ip::ClientIp, rate_limit::enforce},
    state::AppState,
};

async fn log_admin_action(
    state: &AppState,
    admin_sub: &str,
    action: &str,
    lettering_id: Option<Uuid>,
    metadata: serde_json::Value,
) {
    if sqlx::query(
        "INSERT INTO admin_audit_logs (id, admin_sub, action, lettering_id, metadata) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(admin_sub)
    .bind(action)
    .bind(lettering_id)
    .bind(metadata)
    .execute(&state.db)
    .await.is_err()
    {
        tracing::error!(lettering_id = ?lettering_id, "Failed to record admin audit action");
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
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: i64,
}

fn default_status() -> String {
    "ALL".to_string()
}

#[derive(Debug, Deserialize)]
pub struct AuditLogsQuery {
    pub action: Option<String>,
    pub country_code: Option<String>,
    pub lettering_id: Option<Uuid>,
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Debug, Serialize, TS)]
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
    #[ts(type = "Array<string>")]
    pub report_reasons: serde_json::Value,
    pub cultural_context: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, TS)]
pub struct ModerationQueueResponse {
    pub items: Vec<ModerationItem>,
    #[ts(type = "number")]
    pub total: i64,
}

#[derive(Debug, Serialize, FromRow, TS)]
pub struct AdminAuditLogItem {
    pub id: Uuid,
    pub admin_sub: String,
    pub action: String,
    pub lettering_id: Option<Uuid>,
    #[ts(type = "Record<string, unknown>")]
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, TS)]
pub struct AdminAuditLogsResponse {
    pub items: Vec<AdminAuditLogItem>,
    #[ts(type = "number")]
    pub total: i64,
    #[ts(type = "number")]
    pub limit: i64,
    #[ts(type = "number")]
    pub offset: i64,
}

#[derive(Debug, Serialize, TS)]
pub struct StatsResponse {
    #[ts(type = "number")]
    pub total_uploads: i64,
    #[ts(type = "number")]
    pub pending_approvals: i64,
    #[ts(type = "number")]
    pub approved: i64,
    #[ts(type = "number")]
    pub rejected: i64,
    #[ts(type = "number")]
    pub total_cities: i64,
    #[ts(type = "number")]
    pub total_likes: i64,
    #[ts(type = "number")]
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

#[derive(Debug, Serialize, TS)]
pub struct BulkActionFailure {
    pub id: Uuid,
    pub error: String,
}

#[derive(Debug, Serialize, TS)]
pub struct BulkActionResponse {
    pub requested: usize,
    pub processed: usize,
    pub failed: usize,
    pub failed_items: Vec<BulkActionFailure>,
}

// --- Handlers ---

pub async fn login(
    State(state): State<AppState>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    BoundedJson(body): BoundedJson<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    enforce(
        &state,
        &format!("admin-login:{ip}"),
        &state.config.limits.security.admin_login,
    )
    .await?;
    // Validate email
    if body.email != state.config.admin_email {
        return Err(AppError::Forbidden("Invalid credentials".to_string()));
    }

    // Verify password against bcrypt hash
    let valid = state
        .passwords
        .verify(body.password, state.config.admin_password_hash.clone())
        .await?;

    if !valid {
        return Err(AppError::Forbidden("Invalid credentials".to_string()));
    }

    // Issue JWT valid for 24 hours, signed with the dedicated admin secret
    let exp = (chrono::Utc::now()
        + chrono::Duration::seconds(state.config.limits.auth.admin_token_seconds.into()))
    .timestamp() as usize;
    let claims = AdminClaims {
        sub: body.email.clone(),
        exp,
        aud: "admin".to_string(),
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.admin_jwt_secret.as_bytes()),
    )
    .map_err(AppError::internal)?;

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
    let page = crate::presentation::http::pagination::Page::from_limits(
        params.limit,
        params.offset,
        &state.config.limits.pages.moderation,
        &state.config.limits.pagination,
    )?;

    let status_filter = params.status.to_uppercase();
    let safe_limit = page.limit();
    let safe_offset = page.offset();

    let (items, total) = if status_filter == "ALL" {
        let items = sqlx::query_as!(
            ModerationItem,
            r#"SELECT id, image_url, thumbnail_small, contributor_tag, pin_code,
               detected_text, description, status, likes_count, comments_count,
               report_count, report_reasons, cultural_context, created_at
               FROM letterings
               ORDER BY created_at DESC, id DESC
               LIMIT $1 OFFSET $2"#,
            safe_limit,
            safe_offset,
        )
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?;

        let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM letterings")
            .fetch_one(&state.db)
            .await
            .map_err(AppError::internal)?;

        (items, total)
    } else {
        let items = sqlx::query_as!(
            ModerationItem,
            r#"SELECT id, image_url, thumbnail_small, contributor_tag, pin_code,
               detected_text, description, status, likes_count, comments_count,
               report_count, report_reasons, cultural_context, created_at
               FROM letterings
               WHERE status = $1
               ORDER BY created_at ASC, id ASC
               LIMIT $2 OFFSET $3"#,
            status_filter,
            safe_limit,
            safe_offset,
        )
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?;

        let total =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM letterings WHERE status = $1")
                .bind(status_filter)
                .fetch_one(&state.db)
                .await
                .map_err(AppError::internal)?;

        (items, total)
    };

    Ok(Json(ModerationQueueResponse { items, total }))
}

pub async fn approve_lettering(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    apply_lettering_action(&state, id, &claims.sub, LetteringAction::Approve, "", false).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reject_lettering(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
    LimitedJson(body): LimitedJson<RejectRequest>,
) -> Result<StatusCode, AppError> {
    let reason = body.reason.as_deref().unwrap_or("Rejected by admin");
    apply_lettering_action(
        &state,
        id,
        &claims.sub,
        LetteringAction::Reject,
        reason,
        false,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_any_lettering(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    delete_as_admin(&state, id, &claims.sub, "DELETE_LETTERING").await?;
    Ok(StatusCode::NO_CONTENT)
}

/// "Keep & Clear": Resets report_count to 0, clears reasons, restores status to APPROVED
pub async fn clear_reports(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    apply_lettering_action(&state, id, &claims.sub, LetteringAction::Keep, "", false).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_as_admin(
    state: &AppState,
    id: Uuid,
    actor: &str,
    action: &str,
) -> Result<(), AppError> {
    let mut tx = state.db.begin().await?;
    // Account before upload matches account deletion's lock order.
    let locked_owner = sqlx::query_scalar::<_, Uuid>("SELECT u.id FROM users u JOIN letterings l ON l.user_id=u.id WHERE l.id=$1 AND u.deleted_at IS NULL FOR KEY SHARE OF u")
        .bind(id).fetch_optional(&mut *tx).await?;
    let owner = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT user_id FROM letterings WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("Lettering not found".into()))?;
    if let Some(owner) = owner {
        if Some(owner) != locked_owner {
            return Err(AppError::Conflict(
                "Upload ownership changed before deletion".into(),
            ));
        }
        sqlx::query("INSERT INTO notifications(id,user_id,type,title,body,metadata) VALUES($1,$2,'MODERATION_DELETED','Your upload was deleted','Your lettering contribution was removed by moderation.',$3)")
            .bind(Uuid::now_v7()).bind(owner).bind(serde_json::json!({"lettering_id":id})).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO admin_audit_logs(id,admin_sub,action,lettering_id,metadata) VALUES($1,$2,$3,$4,'{}'::jsonb)")
        .bind(Uuid::now_v7()).bind(actor).bind(action).bind(id).execute(&mut *tx).await?;
    sqlx::query!("DELETE FROM letterings WHERE id = $1", id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    state.cache.invalidate().await;
    Ok(())
}

pub async fn get_stats(State(state): State<AppState>) -> Result<Json<StatsResponse>, AppError> {
    // Single round-trip: aggregate all lettering stats in one pass, join city/like/comment counts.
    let row: (i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT
            COUNT(*)                                              AS total_uploads,
            COUNT(*) FILTER (WHERE status = 'PENDING')           AS pending_approvals,
            COUNT(*) FILTER (WHERE status = 'APPROVED')          AS approved,
            COUNT(*) FILTER (WHERE status = 'REJECTED')          AS rejected,
            (SELECT COUNT(*)::bigint FROM cities)                 AS total_cities,
            (SELECT COUNT(*)::bigint FROM likes)                  AS total_likes,
            (SELECT COUNT(*)::bigint FROM comments)               AS total_comments
           FROM letterings"#,
    )
    .fetch_one(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(Json(StatsResponse {
        total_uploads: row.0,
        pending_approvals: row.1,
        approved: row.2,
        rejected: row.3,
        total_cities: row.4,
        total_likes: row.5,
        total_comments: row.6,
    }))
}

pub async fn list_audit_logs(
    State(state): State<AppState>,
    Query(params): Query<AuditLogsQuery>,
) -> Result<Json<AdminAuditLogsResponse>, AppError> {
    let page = crate::presentation::http::pagination::Page::from_limits(
        params.limit,
        params.offset,
        &state.config.limits.pages.moderation,
        &state.config.limits.pagination,
    )?;

    let safe_limit = page.limit();
    let safe_offset = page.offset();
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
        .push(" ORDER BY created_at DESC, id DESC LIMIT ")
        .push_bind(safe_limit)
        .push(" OFFSET ")
        .push_bind(safe_offset);

    let items: Vec<AdminAuditLogItem> = data_qb
        .build_query_as()
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?;

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
        .map_err(AppError::internal)?;

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
    LimitedJson(body): LimitedJson<BulkLetteringActionRequest>,
) -> Result<Json<BulkActionResponse>, AppError> {
    let action = match body.action.trim().to_lowercase().as_str() {
        "approve" => LetteringAction::Approve,
        "reject" => LetteringAction::Reject,
        "keep" => LetteringAction::Keep,
        "delete" => LetteringAction::Delete,
        _ => {
            return Err(AppError::BadRequest(
                "action must be one of approve, reject, delete, keep".into(),
            ));
        }
    };
    if body.ids.is_empty() {
        return Err(AppError::BadRequest("ids cannot be empty".into()));
    }
    if body.ids.len() > 200 {
        return Err(AppError::BadRequest(
            "bulk actions are limited to 200 items".into(),
        ));
    }
    let reason = body
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Bulk moderation action");
    let mut processed = 0;
    let mut failed_items = Vec::new();
    for id in body.ids.iter().copied() {
        match apply_lettering_action(&state, id, &claims.sub, action, reason, true).await {
            Ok(()) => processed += 1,
            Err(error) => failed_items.push(BulkActionFailure {
                id,
                error: error.user_message(),
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

#[derive(Clone, Copy)]
enum LetteringAction {
    Approve,
    Reject,
    Keep,
    Delete,
}

impl LetteringAction {
    fn audit(self, bulk: bool) -> &'static str {
        match (self, bulk) {
            (Self::Approve, false) => "APPROVE_LETTERING",
            (Self::Approve, true) => "BULK_APPROVE_LETTERING",
            (Self::Reject, false) => "REJECT_LETTERING",
            (Self::Reject, true) => "BULK_REJECT_LETTERING",
            (Self::Keep, false) => "CLEAR_REPORTS",
            (Self::Keep, true) => "BULK_CLEAR_REPORTS",
            (Self::Delete, false) => "DELETE_LETTERING",
            (Self::Delete, true) => "BULK_DELETE_LETTERING",
        }
    }
}

/// Commit each decision with its audit and owner notification, including in a bulk request.
async fn apply_lettering_action(
    state: &AppState,
    id: Uuid,
    actor: &str,
    action: LetteringAction,
    reason: &str,
    bulk: bool,
) -> Result<(), AppError> {
    let (status, moderation_reason, notification, title, body) = match action {
        LetteringAction::Approve => (
            "APPROVED",
            if bulk {
                "Approved by bulk moderation"
            } else {
                "Approved by moderation"
            },
            "MODERATION_APPROVED",
            "Your upload was approved",
            "Your lettering contribution has been approved and is now publicly visible.",
        ),
        LetteringAction::Reject => (
            "REJECTED",
            reason,
            "MODERATION_REJECTED",
            "Your upload was rejected",
            "Your lettering contribution was rejected by moderation.",
        ),
        LetteringAction::Keep => (
            "APPROVED",
            "Reports cleared after moderator review",
            "REPORTS_CLEARED",
            "Reports cleared on your upload",
            "Moderator reviewed and cleared reports on your lettering contribution.",
        ),
        LetteringAction::Delete => {
            return delete_as_admin(state, id, actor, action.audit(bulk)).await;
        }
    };
    let mut tx = state.db.begin().await?;
    // Lock the account before the upload, matching account deletion and metadata edits.
    let locked_owner = sqlx::query_scalar::<_, Uuid>("SELECT u.id FROM users u JOIN letterings l ON l.user_id=u.id WHERE l.id=$1 AND u.deleted_at IS NULL FOR KEY SHARE OF u")
        .bind(id).fetch_optional(&mut *tx).await?;
    let owner = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT user_id FROM letterings WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("Lettering not found".into()))?;
    if owner.is_some() && owner != locked_owner {
        return Err(AppError::Conflict(
            "Upload ownership changed before moderation".into(),
        ));
    }
    sqlx::query("UPDATE letterings SET status=$2,moderation_reason=$3,moderated_by=$4,moderated_at=NOW(),updated_at=NOW(),report_count=CASE WHEN $5 THEN 0 ELSE report_count END,report_reasons=CASE WHEN $5 THEN '[]'::jsonb ELSE report_reasons END WHERE id=$1")
        .bind(id).bind(status).bind(moderation_reason).bind(actor).bind(matches!(action, LetteringAction::Keep))
        .execute(&mut *tx).await?;
    let metadata = if matches!(action, LetteringAction::Reject) {
        serde_json::json!({"reason":reason})
    } else {
        serde_json::json!({})
    };
    sqlx::query("INSERT INTO admin_audit_logs(id,admin_sub,action,lettering_id,metadata) VALUES($1,$2,$3,$4,$5)")
        .bind(Uuid::now_v7()).bind(actor).bind(action.audit(bulk)).bind(id).bind(&metadata)
        .execute(&mut *tx).await?;
    if let Some(owner) = owner {
        let mut metadata = metadata;
        metadata["lettering_id"] = serde_json::json!(id);
        sqlx::query("INSERT INTO notifications(id,user_id,type,title,body,metadata) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(Uuid::now_v7()).bind(owner).bind(notification).bind(title).bind(body).bind(metadata)
            .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    state.cache.invalidate().await;
    Ok(())
}
