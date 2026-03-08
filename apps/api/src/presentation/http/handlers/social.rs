use crate::domain::social::repository::SocialRepository;
use crate::infrastructure::security::comment_moderator::assess_comment_content;
use crate::presentation::http::{
    errors::AppError,
    middleware::{
        rate_limit::extract_client_ip,
        user::{decode_optional_user_claims, decode_required_user_claims},
    },
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::Deserialize;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct AddCommentRequest {
    pub content: String,
}

fn apply_region_moderation_policy(
    mut assessment: crate::infrastructure::security::comment_moderator::CommentModerationAssessment,
    level: &str,
) -> crate::infrastructure::security::comment_moderator::CommentModerationAssessment {
    let normalized = level.trim().to_lowercase();
    let has_severe = assessment
        .moderation_flags
        .iter()
        .any(|f| f.starts_with("SEVERE"));

    match normalized.as_str() {
        "strict" => {
            if assessment.status == "VISIBLE" && assessment.moderation_score >= 60 {
                assessment.status = "HIDDEN".to_string();
                assessment.auto_flagged = true;
                assessment.needs_review = true;
                assessment.review_priority = assessment.review_priority.max(85);
                assessment.moderated_by = Some("AUTO_MODERATOR".to_string());
                if assessment.moderation_reason.is_none() {
                    assessment.moderation_reason = Some(
                        "Auto-hidden under strict regional moderation policy".to_string(),
                    );
                }
            } else if assessment.moderation_score >= 25 {
                assessment.needs_review = true;
                assessment.review_priority = assessment.review_priority.max(55);
                if assessment.moderation_reason.is_none() {
                    assessment.moderation_reason = Some(
                        "Flagged for manual review under strict regional moderation policy"
                            .to_string(),
                    );
                }
            }
        }
        "relaxed" => {
            if assessment.status == "HIDDEN" && !has_severe && assessment.moderation_score < 90 {
                assessment.status = "VISIBLE".to_string();
                assessment.auto_flagged = false;
                assessment.needs_review = true;
                assessment.review_priority = assessment.review_priority.max(65);
                assessment.moderated_by = None;
                assessment.moderation_reason = Some(
                    "Visible under relaxed regional policy but queued for review".to_string(),
                );
            }
        }
        _ => {}
    }

    assessment
}

pub async fn like_lettering(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let ip = extract_client_ip(&headers);

    // Use user_id for authenticated requests so likes are per-account, not per-IP.
    // Anonymous users still get IP-based deduplication as before.
    let user_id = decode_optional_user_claims(&headers, &state.config.jwt_secret)
        .and_then(|c| Uuid::parse_str(&c.sub).ok());

    let (liked, count) = state
        .social_repo
        .toggle_like(id, user_id, &ip)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "liked": liked, "likes_count": count })))
}

pub async fn add_comment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<AddCommentRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let claims = decode_required_user_claims(&headers, &state.config.jwt_secret)?;
    let user_id = Uuid::from_str(&claims.sub)
        .map_err(|_| AppError::Forbidden("Invalid token subject".to_string()))?;

    let content = body.content.as_str();

    if content.trim().is_empty() {
        return Err(AppError::BadRequest("Comment cannot be empty".into()));
    }
    if content.len() > 500 {
        return Err(AppError::BadRequest(
            "Comment must be 500 characters or less".into(),
        ));
    }

    let region_policy = sqlx::query_as::<_, (bool, String)>(
        "SELECT COALESCE(rp.comments_enabled, true) AS comments_enabled,
                COALESCE(rp.auto_moderation_level, 'standard') AS auto_moderation_level
         FROM letterings l
         JOIN cities c ON c.id = l.city_id
         LEFT JOIN region_policies rp ON rp.country_code = c.country_code
         WHERE l.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
    .ok_or_else(|| AppError::NotFound("Lettering not found".to_string()))?;

    if !region_policy.0 {
        return Err(AppError::Forbidden(
            "Comments are disabled for this region".to_string(),
        ));
    }

    let ip = extract_client_ip(&headers);

    // Rate limit: 1 comment per 30s per user per lettering
    if let Ok(mut conn) = state.redis.get_multiplexed_async_connection().await {
        let key = format!("comment_rate:{}:{}:{}", id, user_id, ip);
        let exists: bool = redis::cmd("EXISTS")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .unwrap_or(false);
        if exists {
            return Err(AppError::BadRequest(
                "Please wait before commenting again".into(),
            ));
        }
        let _: Result<(), _> = redis::cmd("SET")
            .arg(&key)
            .arg("1")
            .arg("EX")
            .arg(30)
            .query_async(&mut conn)
            .await;
    }

    let comment = state
        .social_repo
        .add_comment(id, user_id, content.to_string(), Some(&ip), {
            let assessment =
                apply_region_moderation_policy(assess_comment_content(content), &region_policy.1);
            crate::domain::social::comment::CommentModerationInput {
                status: assessment.status,
                moderation_score: assessment.moderation_score,
                moderation_flags: assessment.moderation_flags,
                auto_flagged: assessment.auto_flagged,
                needs_review: assessment.needs_review,
                review_priority: assessment.review_priority,
                moderated_by: assessment.moderated_by,
                moderation_reason: assessment.moderation_reason,
            }
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Notify the lettering owner about the new comment (best-effort, non-blocking).
    // Skip notification if the commenter is the owner.
    {
        let db = state.db.clone();
        let lettering_id = id;
        let commenter_id = user_id;
        let content_preview: String = content.chars().take(100).collect();
        tokio::spawn(async move {
            let owner: Option<Uuid> =
                sqlx::query_scalar::<_, Option<Uuid>>("SELECT user_id FROM letterings WHERE id = $1")
                    .bind(lettering_id)
                    .fetch_optional(&db)
                    .await
                    .ok()
                    .flatten()
                    .flatten();
            if let Some(owner_id) = owner
                && owner_id != commenter_id
            {
                let _ = sqlx::query(
                    "INSERT INTO notifications (id, user_id, type, title, body, metadata) \
                     VALUES ($1, $2, 'new_comment', 'New comment on your lettering', $3, $4)",
                )
                .bind(Uuid::now_v7())
                .bind(owner_id)
                .bind(&content_preview)
                .bind(serde_json::json!({ "lettering_id": lettering_id }))
                .execute(&db)
                .await;
            }
        });
    }

    let value = serde_json::to_value(comment)
        .map_err(|e| AppError::Internal(format!("Serialization error: {}", e)))?;
    Ok(Json(value))
}

pub async fn get_comments(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let comments = state
        .social_repo
        .get_comments(id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let value = serde_json::to_value(comments)
        .map_err(|e| AppError::Internal(format!("Serialization error: {}", e)))?;
    Ok(Json(value))
}
