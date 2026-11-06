use crate::{limits::PaginationLimits, presentation::http::errors::AppError};

/// Validated bounds for a single database page.
#[derive(Debug, Clone, Copy)]
pub struct Page {
    limit: i64,
    offset: i64,
}

impl Page {
    pub fn new(
        limit: i64,
        offset: i64,
        max_limit: i64,
        policy: &PaginationLimits,
    ) -> Result<Self, AppError> {
        if !(1..=max_limit).contains(&limit) {
            return Err(AppError::BadRequest(format!(
                "limit must be between 1 and {max_limit}"
            )));
        }
        if !(0..=policy.max_offset).contains(&offset) {
            return Err(AppError::BadRequest(format!(
                "offset must be between 0 and {}",
                policy.max_offset
            )));
        }
        Ok(Self { limit, offset })
    }

    pub fn limit(self) -> i64 {
        self.limit
    }
    pub fn offset(self) -> i64 {
        self.offset
    }
}
