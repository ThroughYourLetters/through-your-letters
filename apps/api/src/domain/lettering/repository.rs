use super::entity::Lettering;
use super::errors::DomainError;
use async_trait::async_trait;
use std::num::NonZeroU32;
use uuid::Uuid;

pub struct NewUpload<'a> {
    pub lettering: &'a Lettering,
    pub owner_id: Option<Uuid>,
    pub analysis_attempts: Option<NonZeroU32>,
    pub request: Option<UploadRequest<'a>>,
}

pub struct UploadRequest<'a> {
    pub key: Uuid,
    pub payload_hash: &'a str,
}

#[async_trait]
pub trait LetteringRepository: Send + Sync {
    async fn create(&self, upload: NewUpload<'_>) -> Result<Lettering, DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Lettering>, DomainError>;

    async fn update(&self, lettering: &Lettering) -> Result<Lettering, DomainError>;

    async fn find_by_image_hash(&self, hash: &str) -> Result<Option<Lettering>, DomainError>;
    async fn find_by_contributor(
        &self,
        tag: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Lettering>, DomainError>;
    async fn count_by_contributor(&self, tag: &str) -> Result<i64, DomainError>;
}
