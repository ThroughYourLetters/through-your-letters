use super::super::blocking::{BlockingError, BlockingPool};
use crate::limits::PasswordLimits;
use bcrypt::{DEFAULT_COST, HashParts};
use std::{str::FromStr, time::Duration};

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("Password must contain at least 8 characters and at most 72 UTF-8 bytes")]
    InvalidNewPassword,
    #[error("Password exceeds the sign-in length limit")]
    TooLong,
    #[error("Stored password hash is unsupported")]
    UnsupportedHash,
    #[error(transparent)]
    Work(#[from] BlockingError),
    #[error("Password processing failed")]
    Processing,
}

pub struct PasswordService {
    pool: BlockingPool,
    max_verify_bytes: usize,
    max_verify_cost: u32,
}

impl PasswordService {
    pub fn new(limits: &PasswordLimits) -> Self {
        Self {
            pool: BlockingPool::new(
                limits.workers,
                Duration::from_millis(limits.work_timeout_ms),
            ),
            max_verify_bytes: limits.max_verify_bytes,
            max_verify_cost: limits.max_verify_cost,
        }
    }

    pub async fn hash(&self, password: String) -> Result<String, PasswordError> {
        if password.len() > 72 || password.chars().count() < 8 {
            return Err(PasswordError::InvalidNewPassword);
        }
        // bcrypt 0.18's non_truncating_hash also counts its appended NUL and
        // rejects exactly 72 bytes. The check above bounds the password itself.
        self.pool
            .run(move || bcrypt::hash(password, DEFAULT_COST))
            .await?
            .map_err(|_| PasswordError::Processing)
    }

    pub async fn verify(&self, password: String, hash: String) -> Result<bool, PasswordError> {
        if password.len() > self.max_verify_bytes {
            return Err(PasswordError::TooLong);
        }
        let parts = HashParts::from_str(&hash).map_err(|_| PasswordError::UnsupportedHash)?;
        if parts.get_cost() > self.max_verify_cost {
            return Err(PasswordError::UnsupportedHash);
        }
        // Existing bcrypt hashes may represent passwords longer than 72 bytes.
        // Preserve their verification behavior while rejecting new truncated hashes.
        self.pool
            .run(move || bcrypt::verify(password, &hash))
            .await?
            .map_err(|_| PasswordError::Processing)
    }
}
