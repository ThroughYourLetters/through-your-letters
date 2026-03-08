use super::comment::{Comment, CommentModerationInput};
use crate::domain::lettering::errors::DomainError;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait SocialRepository: Send + Sync {
    /// Toggle a like for a lettering.
    ///
    /// Authenticated callers supply `user_id`; anonymous callers supply only `user_ip`.
    /// When `user_id` is `Some`, it is the primary deduplication key (one like per user per
    /// lettering). When `None`, `user_ip` is used as the fallback key.
    async fn toggle_like(
        &self,
        lettering_id: Uuid,
        user_id: Option<Uuid>,
        user_ip: &str,
    ) -> Result<(bool, i32), DomainError>;
    async fn add_comment(
        &self,
        lettering_id: Uuid,
        user_id: Uuid,
        content: String,
        user_ip: Option<&str>,
        moderation: CommentModerationInput,
    ) -> Result<Comment, DomainError>;
    async fn get_comments(&self, lettering_id: Uuid) -> Result<Vec<Comment>, DomainError>;
    async fn has_liked(&self, lettering_id: Uuid, user_ip: &str) -> Result<bool, DomainError>;
    async fn get_likes_count(&self, lettering_id: Uuid) -> Result<i32, DomainError>;
}
