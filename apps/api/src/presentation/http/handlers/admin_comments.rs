use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};
use ts_rs::TS;
use uuid::Uuid;

use crate::presentation::http::{
    errors::AppError, middleware::admin::AdminClaims, state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CommentsQuery {
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub q: Option<String>,
    pub needs_review: Option<bool>,
    pub min_score: Option<i32>,
    pub sort: Option<String>,
}

fn default_status() -> String {
    "ALL".to_string()
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Serialize, FromRow, TS)]
pub struct AdminCommentItem {
    pub id: Uuid,
    pub lettering_id: Uuid,
    pub content: String,
    pub commenter_name: Option<String>,
    pub commenter_email: Option<String>,
    pub status: String,
    pub moderation_score: i32,
    #[ts(type = "Array<string>")]
    pub moderation_flags: serde_json::Value,
    pub auto_flagged: bool,
    pub needs_review: bool,
    pub review_priority: i32,
    pub moderated_by: Option<String>,
    pub moderation_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub pin_code: String,
    pub contributor_tag: String,
    pub lettering_image_url: String,
    pub lettering_thumbnail: String,
}

#[derive(Debug, Serialize, TS)]
pub struct AdminCommentsResponse {
    pub items: Vec<AdminCommentItem>,
    #[ts(type = "number")]
    pub total: i64,
    #[ts(type = "number")]
    pub limit: i64,
    #[ts(type = "number")]
    pub offset: i64,
}

#[derive(Debug, Deserialize)]
pub struct HideCommentRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BulkCommentActionRequest {
    pub ids: Vec<Uuid>,
    pub action: String,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct BulkCommentActionFailure {
    pub id: Uuid,
    pub error: String,
}

#[derive(Debug, Serialize, TS)]
pub struct BulkCommentActionResponse {
    pub requested: usize,
    pub processed: usize,
    pub failed: usize,
    pub failed_items: Vec<BulkCommentActionFailure>,
}

#[derive(Debug, Clone, Copy)]
enum CommentAction {
    Hide,
    Restore,
    Delete,
}

impl CommentAction {
    fn parse(value: &str) -> Result<Self, AppError> {
        match value.trim().to_lowercase().as_str() {
            "hide" => Ok(Self::Hide),
            "restore" => Ok(Self::Restore),
            "delete" => Ok(Self::Delete),
            _ => Err(AppError::BadRequest(
                "action must be one of hide, restore, delete".into(),
            )),
        }
    }
    fn audit(self, bulk: bool) -> &'static str {
        match (self, bulk) {
            (Self::Hide, false) => "HIDE_COMMENT",
            (Self::Hide, true) => "BULK_HIDE_COMMENT",
            (Self::Restore, false) => "RESTORE_COMMENT",
            (Self::Restore, true) => "BULK_RESTORE_COMMENT",
            (Self::Delete, false) => "DELETE_COMMENT",
            (Self::Delete, true) => "BULK_DELETE_COMMENT",
        }
    }
    fn notice(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Hide => (
                "COMMENT_HIDDEN",
                "Your comment was hidden",
                "A moderator hid one of your comments due to policy concerns.",
            ),
            Self::Restore => (
                "COMMENT_RESTORED",
                "Your comment was restored",
                "A moderator restored your comment.",
            ),
            Self::Delete => (
                "COMMENT_DELETED",
                "Your comment was deleted",
                "A moderator removed one of your comments.",
            ),
        }
    }
}

fn hide_reason(reason: Option<&str>) -> &str {
    reason
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Hidden by moderation")
}

async fn apply_comment_action(
    state: &AppState,
    claims: &AdminClaims,
    id: Uuid,
    action: CommentAction,
    reason: &str,
    bulk: bool,
) -> Result<(), AppError> {
    let mut tx = state.db.begin().await?;
    let parent: Uuid = sqlx::query_scalar("SELECT lettering_id FROM comments WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("Comment not found".into()))?;
    // All social writers lock the parent before changing children or counters.
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM letterings WHERE id=$1 FOR UPDATE")
        .bind(parent)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("Lettering not found".into()))?;
    let owner: Option<Uuid> = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT user_id FROM comments WHERE id=$1 AND lettering_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(parent)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("Comment not found".into()))?;
    match action {
        CommentAction::Hide => {
            sqlx::query("UPDATE comments SET status='HIDDEN',needs_review=false,moderated_at=NOW(),moderated_by=$2,moderation_reason=$3,updated_at=NOW() WHERE id=$1")
                .bind(id).bind(&claims.sub).bind(reason).execute(&mut *tx).await?;
        }
        CommentAction::Restore => {
            sqlx::query("UPDATE comments SET status='VISIBLE',needs_review=false,moderated_at=NULL,moderated_by=NULL,moderation_reason=NULL,updated_at=NOW() WHERE id=$1")
                .bind(id).execute(&mut *tx).await?;
        }
        CommentAction::Delete => {
            sqlx::query("DELETE FROM comments WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    crate::infrastructure::repositories::sqlx_social_repository::refresh_comments_count(
        &mut tx, parent,
    )
    .await?;
    let metadata = if matches!(action, CommentAction::Hide) {
        serde_json::json!({ "comment_id": id, "reason": reason })
    } else {
        serde_json::json!({ "comment_id": id })
    };
    sqlx::query("INSERT INTO admin_audit_logs(id,admin_sub,action,lettering_id,metadata,created_at) VALUES($1,$2,$3,$4,$5,NOW())")
        .bind(Uuid::now_v7()).bind(&claims.sub).bind(action.audit(bulk)).bind(parent).bind(&metadata)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    state.cache.invalidate().await;
    // Notification delivery is best-effort after commit; it cannot announce rollback.
    if let Some(owner) = owner {
        let (kind, title, body) = action.notice();
        if sqlx::query("INSERT INTO notifications(id,user_id,type,title,body,metadata) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(Uuid::now_v7()).bind(owner).bind(kind).bind(title).bind(body).bind(metadata)
            .execute(&state.db).await.is_err() {
            tracing::warn!(action = ?action, "Comment notification failed after moderation committed");
        }
    }
    Ok(())
}

pub async fn list_comments(
    State(state): State<AppState>,
    Query(params): Query<CommentsQuery>,
) -> Result<Json<AdminCommentsResponse>, AppError> {
    let status = params.status.to_uppercase();
    if !["ALL", "VISIBLE", "HIDDEN"].contains(&status.as_str()) {
        return Err(AppError::BadRequest(
            "status must be one of ALL, VISIBLE, HIDDEN".to_string(),
        ));
    }

    let q = params.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let sort = params.sort.as_deref().unwrap_or("priority");
    if !["priority", "newest", "score"].contains(&sort) {
        return Err(AppError::BadRequest(
            "sort must be one of priority, newest, score".to_string(),
        ));
    }

    let mut items_qb = QueryBuilder::<Postgres>::new(
        "SELECT c.id, c.lettering_id, c.content,
                COALESCE(NULLIF(u.display_name, ''), u.email, 'Anonymous') AS commenter_name,
                u.email AS commenter_email,
                c.status, c.moderation_score, c.moderation_flags, c.auto_flagged, c.needs_review, c.review_priority,
                c.moderated_by, c.moderation_reason,
                c.created_at, c.updated_at,
                l.pin_code, l.contributor_tag, l.image_url AS lettering_image_url,
                l.thumbnail_small AS lettering_thumbnail
         FROM comments c
         JOIN letterings l ON l.id = c.lettering_id
         LEFT JOIN users u ON u.id = c.user_id
         WHERE 1=1",
    );

    if status != "ALL" {
        items_qb.push(" AND c.status = ").push_bind(&status);
    }

    if let Some(search) = q {
        let like = format!("%{}%", search);
        items_qb.push(" AND (c.content ILIKE ");
        items_qb.push_bind(like.clone());
        items_qb.push(" OR COALESCE(u.display_name, '') ILIKE ");
        items_qb.push_bind(like.clone());
        items_qb.push(" OR COALESCE(u.email, '') ILIKE ");
        items_qb.push_bind(like);
        items_qb.push(")");
    }

    if let Some(needs_review) = params.needs_review {
        items_qb
            .push(" AND c.needs_review = ")
            .push_bind(needs_review);
    }

    if let Some(min_score) = params.min_score {
        items_qb
            .push(" AND c.moderation_score >= ")
            .push_bind(min_score.max(0));
    }
    match sort {
        "newest" => {
            items_qb.push(" ORDER BY c.created_at DESC");
        }
        "score" => {
            items_qb.push(" ORDER BY c.moderation_score DESC, c.created_at DESC");
        }
        _ => {
            items_qb.push(
                " ORDER BY c.review_priority DESC, c.auto_flagged DESC, c.moderation_score DESC, c.created_at DESC",
            );
        }
    }
    items_qb
        .push(" LIMIT ")
        .push_bind(params.limit.clamp(1, 200))
        .push(" OFFSET ")
        .push_bind(params.offset.max(0));

    let items: Vec<AdminCommentItem> = items_qb
        .build_query_as()
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?;

    let mut count_qb = QueryBuilder::<Postgres>::new(
        "SELECT COUNT(*)::bigint as total FROM comments c LEFT JOIN users u ON u.id = c.user_id WHERE 1=1",
    );

    if status != "ALL" {
        count_qb.push(" AND c.status = ").push_bind(&status);
    }

    if let Some(search) = q {
        let like = format!("%{}%", search);
        count_qb.push(" AND (c.content ILIKE ");
        count_qb.push_bind(like.clone());
        count_qb.push(" OR COALESCE(u.display_name, '') ILIKE ");
        count_qb.push_bind(like.clone());
        count_qb.push(" OR COALESCE(u.email, '') ILIKE ");
        count_qb.push_bind(like);
        count_qb.push(")");
    }

    if let Some(needs_review) = params.needs_review {
        count_qb
            .push(" AND c.needs_review = ")
            .push_bind(needs_review);
    }

    if let Some(min_score) = params.min_score {
        count_qb
            .push(" AND c.moderation_score >= ")
            .push_bind(min_score.max(0));
    }

    let total: i64 = count_qb
        .build_query_scalar()
        .fetch_one(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(Json(AdminCommentsResponse {
        items,
        total,
        limit: params.limit.clamp(1, 200),
        offset: params.offset.max(0),
    }))
}

pub async fn hide_comment(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
    Json(body): Json<HideCommentRequest>,
) -> Result<StatusCode, AppError> {
    apply_comment_action(
        &state,
        &claims,
        id,
        CommentAction::Hide,
        hide_reason(body.reason.as_deref()),
        false,
    )
    .await?;
    Ok(StatusCode::OK)
}

pub async fn restore_comment(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    apply_comment_action(&state, &claims, id, CommentAction::Restore, "", false).await?;
    Ok(StatusCode::OK)
}

pub async fn delete_comment(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    apply_comment_action(&state, &claims, id, CommentAction::Delete, "", false).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn bulk_comment_action(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Json(body): Json<BulkCommentActionRequest>,
) -> Result<Json<BulkCommentActionResponse>, AppError> {
    let action = CommentAction::parse(&body.action)?;
    if body.ids.is_empty() {
        return Err(AppError::BadRequest("ids cannot be empty".into()));
    }
    if body.ids.len() > 200 {
        return Err(AppError::BadRequest(
            "bulk actions are limited to 200 comments".into(),
        ));
    }
    let reason = hide_reason(body.reason.as_deref());
    let mut processed = 0;
    let mut failed_items = Vec::new();
    for id in body.ids.iter().copied() {
        // Every item has its own transaction, preserving the bulk partial-success contract.
        match apply_comment_action(&state, &claims, id, action, reason, true).await {
            Ok(()) => processed += 1,
            Err(error) => failed_items.push(BulkCommentActionFailure {
                id,
                error: error.user_message(),
            }),
        }
    }
    Ok(Json(BulkCommentActionResponse {
        requested: body.ids.len(),
        processed,
        failed: failed_items.len(),
        failed_items,
    }))
}
