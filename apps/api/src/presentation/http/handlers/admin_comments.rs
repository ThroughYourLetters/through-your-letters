use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};
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

#[derive(Debug, Serialize, FromRow)]
pub struct AdminCommentItem {
    pub id: Uuid,
    pub lettering_id: Uuid,
    pub content: String,
    pub commenter_name: Option<String>,
    pub commenter_email: Option<String>,
    pub status: String,
    pub moderation_score: i32,
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

#[derive(Debug, Serialize)]
pub struct AdminCommentsResponse {
    pub items: Vec<AdminCommentItem>,
    pub total: i64,
    pub limit: i64,
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

#[derive(Debug, Serialize)]
pub struct BulkCommentActionFailure {
    pub id: Uuid,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct BulkCommentActionResponse {
    pub requested: usize,
    pub processed: usize,
    pub failed: usize,
    pub failed_items: Vec<BulkCommentActionFailure>,
}

#[derive(Debug, FromRow)]
struct CommentOwnerRow {
    lettering_id: Uuid,
    user_id: Option<Uuid>,
}

async fn recompute_comments_count(state: &AppState, lettering_id: Uuid) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE letterings
         SET comments_count = (
           SELECT COUNT(*)::int FROM comments WHERE lettering_id = $1 AND status = 'VISIBLE'
         )
         WHERE id = $1",
    )
    .bind(lettering_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::InternalError(e.to_string()))?;
    Ok(())
}

async fn log_admin_action(
    state: &AppState,
    admin_sub: &str,
    action: &str,
    metadata: serde_json::Value,
) {
    let _ = sqlx::query(
        "INSERT INTO admin_audit_logs (id, admin_sub, action, metadata, created_at) VALUES ($1, $2, $3, $4, NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(admin_sub)
    .bind(action)
    .bind(metadata)
    .execute(&state.db)
    .await;
}

async fn notify_comment_owner(
    state: &AppState,
    user_id: Option<Uuid>,
    n_type: &str,
    title: &str,
    body: &str,
    metadata: serde_json::Value,
) {
    let Some(owner_id) = user_id else {
        return;
    };

    let _ = sqlx::query(
        "INSERT INTO notifications (id, user_id, type, title, body, metadata) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::now_v7())
    .bind(owner_id)
    .bind(n_type)
    .bind(title)
    .bind(body)
    .bind(metadata)
    .execute(&state.db)
    .await;
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
        .push_bind(params.limit.max(1).min(200))
        .push(" OFFSET ")
        .push_bind(params.offset.max(0));

    let items: Vec<AdminCommentItem> = items_qb
        .build_query_as()
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;

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
        .map_err(|e| AppError::InternalError(e.to_string()))?;

    Ok(Json(AdminCommentsResponse {
        items,
        total,
        limit: params.limit.max(1).min(200),
        offset: params.offset.max(0),
    }))
}

pub async fn hide_comment(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
    Json(body): Json<HideCommentRequest>,
) -> Result<StatusCode, AppError> {
    let owner = sqlx::query_as::<_, CommentOwnerRow>(
        "SELECT lettering_id, user_id FROM comments WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::InternalError(e.to_string()))?
    .ok_or_else(|| AppError::NotFound("Comment not found".to_string()))?;

    let reason = body
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Hidden by moderation");

    sqlx::query(
        "UPDATE comments
         SET status = 'HIDDEN', needs_review = false, moderated_at = NOW(), moderated_by = $2, moderation_reason = $3, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .bind(&claims.sub)
    .bind(reason)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::InternalError(e.to_string()))?;

    recompute_comments_count(&state, owner.lettering_id).await?;

    log_admin_action(
        &state,
        &claims.sub,
        "HIDE_COMMENT",
        serde_json::json!({ "comment_id": id, "reason": reason }),
    )
    .await;
    notify_comment_owner(
        &state,
        owner.user_id,
        "COMMENT_HIDDEN",
        "Your comment was hidden",
        "A moderator hid one of your comments due to policy concerns.",
        serde_json::json!({ "comment_id": id, "reason": reason }),
    )
    .await;

    Ok(StatusCode::OK)
}

pub async fn restore_comment(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let owner = sqlx::query_as::<_, CommentOwnerRow>(
        "SELECT lettering_id, user_id FROM comments WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::InternalError(e.to_string()))?
    .ok_or_else(|| AppError::NotFound("Comment not found".to_string()))?;

    sqlx::query(
        "UPDATE comments
         SET status = 'VISIBLE', needs_review = false, moderated_at = NULL, moderated_by = NULL, moderation_reason = NULL, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::InternalError(e.to_string()))?;

    recompute_comments_count(&state, owner.lettering_id).await?;

    log_admin_action(
        &state,
        &claims.sub,
        "RESTORE_COMMENT",
        serde_json::json!({ "comment_id": id }),
    )
    .await;
    notify_comment_owner(
        &state,
        owner.user_id,
        "COMMENT_RESTORED",
        "Your comment was restored",
        "A moderator restored your comment.",
        serde_json::json!({ "comment_id": id }),
    )
    .await;

    Ok(StatusCode::OK)
}

pub async fn delete_comment(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let owner = sqlx::query_as::<_, CommentOwnerRow>(
        "SELECT lettering_id, user_id FROM comments WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::InternalError(e.to_string()))?
    .ok_or_else(|| AppError::NotFound("Comment not found".to_string()))?;

    sqlx::query("DELETE FROM comments WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;

    recompute_comments_count(&state, owner.lettering_id).await?;

    log_admin_action(
        &state,
        &claims.sub,
        "DELETE_COMMENT",
        serde_json::json!({ "comment_id": id }),
    )
    .await;
    notify_comment_owner(
        &state,
        owner.user_id,
        "COMMENT_DELETED",
        "Your comment was deleted",
        "A moderator removed one of your comments.",
        serde_json::json!({ "comment_id": id }),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn bulk_comment_action(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Json(body): Json<BulkCommentActionRequest>,
) -> Result<Json<BulkCommentActionResponse>, AppError> {
    let action = body.action.trim().to_lowercase();
    if !["hide", "restore", "delete"].contains(&action.as_str()) {
        return Err(AppError::BadRequest(
            "action must be one of hide, restore, delete".to_string(),
        ));
    }
    if body.ids.is_empty() {
        return Err(AppError::BadRequest("ids cannot be empty".to_string()));
    }
    if body.ids.len() > 200 {
        return Err(AppError::BadRequest(
            "bulk actions are limited to 200 comments".to_string(),
        ));
    }

    let reason = body
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Hidden by moderation");

    let mut processed = 0usize;
    let mut failed_items = Vec::new();

    for id in body.ids.iter().copied() {
        let owner = sqlx::query_as::<_, CommentOwnerRow>(
            "SELECT lettering_id, user_id FROM comments WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;

        let Some(owner) = owner else {
            failed_items.push(BulkCommentActionFailure {
                id,
                error: "Comment not found".to_string(),
            });
            continue;
        };

        let result = match action.as_str() {
            "hide" => {
                let update = sqlx::query(
                    "UPDATE comments
                     SET status = 'HIDDEN', needs_review = false, moderated_at = NOW(), moderated_by = $2, moderation_reason = $3, updated_at = NOW()
                     WHERE id = $1",
                )
                .bind(id)
                .bind(&claims.sub)
                .bind(reason)
                .execute(&state.db)
                .await
                .map_err(|e| AppError::InternalError(e.to_string()));

                if update.is_ok() {
                    let _ = recompute_comments_count(&state, owner.lettering_id).await;
                    log_admin_action(
                        &state,
                        &claims.sub,
                        "BULK_HIDE_COMMENT",
                        serde_json::json!({ "comment_id": id, "reason": reason }),
                    )
                    .await;
                    notify_comment_owner(
                        &state,
                        owner.user_id,
                        "COMMENT_HIDDEN",
                        "Your comment was hidden",
                        "A moderator hid one of your comments due to policy concerns.",
                        serde_json::json!({ "comment_id": id, "reason": reason }),
                    )
                    .await;
                }
                update
            }
            "restore" => {
                let update = sqlx::query(
                    "UPDATE comments
                     SET status = 'VISIBLE', needs_review = false, moderated_at = NULL, moderated_by = NULL, moderation_reason = NULL, updated_at = NOW()
                     WHERE id = $1",
                )
                .bind(id)
                .execute(&state.db)
                .await
                .map_err(|e| AppError::InternalError(e.to_string()));

                if update.is_ok() {
                    let _ = recompute_comments_count(&state, owner.lettering_id).await;
                    log_admin_action(
                        &state,
                        &claims.sub,
                        "BULK_RESTORE_COMMENT",
                        serde_json::json!({ "comment_id": id }),
                    )
                    .await;
                    notify_comment_owner(
                        &state,
                        owner.user_id,
                        "COMMENT_RESTORED",
                        "Your comment was restored",
                        "A moderator restored your comment.",
                        serde_json::json!({ "comment_id": id }),
                    )
                    .await;
                }
                update
            }
            _ => {
                let delete = sqlx::query("DELETE FROM comments WHERE id = $1")
                    .bind(id)
                    .execute(&state.db)
                    .await
                    .map_err(|e| AppError::InternalError(e.to_string()));

                if delete.is_ok() {
                    let _ = recompute_comments_count(&state, owner.lettering_id).await;
                    log_admin_action(
                        &state,
                        &claims.sub,
                        "BULK_DELETE_COMMENT",
                        serde_json::json!({ "comment_id": id }),
                    )
                    .await;
                    notify_comment_owner(
                        &state,
                        owner.user_id,
                        "COMMENT_DELETED",
                        "Your comment was deleted",
                        "A moderator removed one of your comments.",
                        serde_json::json!({ "comment_id": id }),
                    )
                    .await;
                }
                delete
            }
        };

        match result {
            Ok(_) => processed += 1,
            Err(err) => {
                let message = match err {
                    AppError::NotFound(msg)
                    | AppError::Forbidden(msg)
                    | AppError::BadRequest(msg)
                    | AppError::ValidationError(msg)
                    | AppError::InternalError(msg) => msg,
                };
                failed_items.push(BulkCommentActionFailure { id, error: message });
            }
        }
    }

    Ok(Json(BulkCommentActionResponse {
        requested: body.ids.len(),
        processed,
        failed: failed_items.len(),
        failed_items,
    }))
}
