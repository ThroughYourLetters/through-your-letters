use super::dto::AddCommentRequest;
use crate::domain::lettering::errors::DomainError;
use crate::domain::social::{
    comment::{Comment, CommentModerationInput},
    repository::SocialRepository,
};
use uuid::Uuid;

pub struct SocialUseCase {
    repository: Box<dyn SocialRepository>,
}

impl SocialUseCase {
    pub fn new(repository: Box<dyn SocialRepository>) -> Self {
        Self { repository }
    }

    pub async fn toggle_like(
        &self,
        lettering_id: Uuid,
        user_ip: &str,
    ) -> Result<(bool, i32), DomainError> {
        // Calls the single toggle method in the repository
        self.repository.toggle_like(lettering_id, user_ip).await
    }

    pub async fn add_comment(
        &self,
        request: AddCommentRequest,
        user_id: Uuid,
        moderation: CommentModerationInput,
        user_ip: Option<&str>,
    ) -> Result<Comment, DomainError> {
        self.repository
            .add_comment(
                request.lettering_id,
                user_id,
                request.content,
                user_ip,
                moderation,
            )
            .await
    }

    pub async fn get_comments(&self, lettering_id: Uuid) -> Result<Vec<Comment>, DomainError> {
        self.repository.get_comments(lettering_id).await
    }
}
