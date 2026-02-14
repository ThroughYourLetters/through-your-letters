use super::dto::PaginatedResponse;
use crate::domain::lettering::{errors::DomainError, repository::LetteringRepository};

pub struct GetLetteringsUseCase {
    repository: Box<dyn LetteringRepository>,
}

impl GetLetteringsUseCase {
    pub fn new(repository: Box<dyn LetteringRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, limit: i64, offset: i64) -> Result<PaginatedResponse, DomainError> {
        let letterings = self.repository.find_all(limit, offset).await?;
        Ok(PaginatedResponse {
            letterings: letterings.clone(),
            total: letterings.len() as i64,
            limit,
            offset,
        })
    }
}
