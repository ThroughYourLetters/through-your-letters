use crate::domain::{
    lettering::errors::DomainError,
    social::{
        comment::{Comment, CommentModerationInput},
        repository::SocialRepository,
    },
};
use async_trait::async_trait;
use sqlx::{PgPool, types::ipnetwork::IpNetwork};
use std::str::FromStr;
use uuid::Uuid;

pub struct SqlxSocialRepository {
    pub pool: PgPool,
}
impl SqlxSocialRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_readable_comments(
        &self,
        lettering_id: Uuid,
        requester: Option<Uuid>,
    ) -> Result<Vec<Comment>, DomainError> {
        let rows = sqlx::query_as::<_, Comment>(
            "SELECT c.id, c.lettering_id, c.content, c.user_id, \
                    COALESCE(NULLIF(BTRIM(u.display_name), ''), 'Anonymous') as commenter_name, \
                    c.status, c.moderation_score, \
                    COALESCE(ARRAY(SELECT jsonb_array_elements_text(c.moderation_flags)), ARRAY[]::text[]) as moderation_flags, \
                    c.auto_flagged, c.needs_review, c.review_priority, \
                    c.user_ip, c.moderated_at, c.moderated_by, c.moderation_reason, c.created_at, c.updated_at \
             FROM comments c \
             LEFT JOIN users u ON u.id = c.user_id \
             WHERE c.lettering_id = $1 AND c.status = 'VISIBLE' AND c.lettering_id IN (SELECT id FROM readable_letterings($2)) \
             ORDER BY c.created_at DESC",
        )
        .bind(lettering_id)
        .bind(requester)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError(e.to_string()))?;
        Ok(rows)
    }
}

#[async_trait]
impl SocialRepository for SqlxSocialRepository {
    async fn toggle_like(
        &self,
        lettering_id: Uuid,
        user_id: Option<Uuid>,
        user_ip: &str,
    ) -> Result<(bool, i32), DomainError> {
        let ip = IpNetwork::from_str(user_ip)
            .map_err(|error| DomainError::ValidationError(error.to_string()))?;
        let mut tx = self.pool.begin().await.map_err(database_error)?;

        // Account deletion takes the user lock first, then contribution locks.
        if let Some(user_id) = user_id {
            let user = sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM users WHERE id = $1 AND deleted_at IS NULL FOR KEY SHARE",
            )
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(database_error)?;
            if user.is_none() {
                return Err(DomainError::Unauthorized);
            }
        }
        let lettering =
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM letterings WHERE id = $1 FOR UPDATE")
                .bind(lettering_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(database_error)?;
        if lettering.is_none() {
            return Err(DomainError::NotFound("Lettering not found".into()));
        }
        // Recheck after acquiring the lock: a competing moderator may have hidden it.
        let readable: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM readable_letterings($2) WHERE id = $1)",
        )
        .bind(lettering_id)
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
        if !readable {
            return Err(DomainError::NotFound("Lettering not found".into()));
        }

        let removed = if let Some(user_id) = user_id {
            sqlx::query("DELETE FROM likes WHERE lettering_id = $1 AND user_id = $2")
                .bind(lettering_id)
                .bind(user_id)
                .execute(&mut *tx)
                .await
        } else {
            sqlx::query(
                "DELETE FROM likes WHERE lettering_id = $1 AND user_ip = $2 AND user_id IS NULL",
            )
            .bind(lettering_id)
            .bind(ip)
            .execute(&mut *tx)
            .await
        }
        .map_err(database_error)?
        .rows_affected();
        let liked = removed == 0;
        if liked {
            sqlx::query(
                "INSERT INTO likes(id, lettering_id, user_id, user_ip) VALUES($1,$2,$3,$4)",
            )
            .bind(Uuid::now_v7())
            .bind(lettering_id)
            .bind(user_id)
            .bind(ip)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
        }
        // Derive the summary from rows under the same parent lock, repairing old drift.
        let count = sqlx::query_scalar::<_, i32>(
            "UPDATE letterings SET likes_count = (SELECT COUNT(*)::int FROM likes WHERE lettering_id = $1) WHERE id = $1 RETURNING likes_count",
        ).bind(lettering_id).fetch_one(&mut *tx).await.map_err(database_error)?;
        tx.commit().await.map_err(database_error)?;
        Ok((liked, count))
    }

    async fn add_comment(
        &self,
        lettering_id: Uuid,
        user_id: Uuid,
        content: String,
        user_ip: Option<&str>,
        moderation: CommentModerationInput,
    ) -> Result<Comment, DomainError> {
        let ip = user_ip.and_then(|i| IpNetwork::from_str(i).ok());
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO comments (
                id, lettering_id, user_id, content, user_ip, status,
                moderation_score, moderation_flags, auto_flagged, needs_review, review_priority,
                moderated_at, moderated_by, moderation_reason
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8::jsonb, $9, $10, $11,
                CASE WHEN $6 = 'HIDDEN' THEN NOW() ELSE NULL END, $12, $13
            )",
        )
        .bind(id)
        .bind(lettering_id)
        .bind(user_id)
        .bind(&content)
        .bind(ip)
        .bind(&moderation.status)
        .bind(moderation.moderation_score)
        .bind(serde_json::to_value(&moderation.moderation_flags).unwrap_or(serde_json::json!([])))
        .bind(moderation.auto_flagged)
        .bind(moderation.needs_review)
        .bind(moderation.review_priority)
        .bind(moderation.moderated_by)
        .bind(moderation.moderation_reason)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError(e.to_string()))?;

        if moderation.status == "VISIBLE" {
            sqlx::query("UPDATE letterings SET comments_count = comments_count + 1 WHERE id = $1")
                .bind(lettering_id)
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::InfrastructureError(e.to_string()))?;
        }

        let row = sqlx::query_as::<_, Comment>(
            "SELECT c.id, c.lettering_id, c.content, c.user_id, \
                    COALESCE(NULLIF(BTRIM(u.display_name), ''), 'Anonymous') as commenter_name, \
                    c.status, c.moderation_score, \
                    COALESCE(ARRAY(SELECT jsonb_array_elements_text(c.moderation_flags)), ARRAY[]::text[]) as moderation_flags, \
                    c.auto_flagged, c.needs_review, c.review_priority, \
                    c.user_ip, c.moderated_at, c.moderated_by, c.moderation_reason, c.created_at, c.updated_at \
             FROM comments c \
             LEFT JOIN users u ON u.id = c.user_id \
             WHERE c.id = $1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError(e.to_string()))?;

        Ok(row)
    }
}

fn database_error(error: sqlx::Error) -> DomainError {
    DomainError::InfrastructureError(error.to_string())
}
