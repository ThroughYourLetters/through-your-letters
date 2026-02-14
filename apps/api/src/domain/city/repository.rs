use super::entity::City;
use crate::domain::lettering::errors::DomainError;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait CityRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<City>, DomainError>;
    async fn find_all(&self) -> Result<Vec<City>, DomainError>;
}
