//! Safe HTTP responses and fixed diagnostic categories for handler failures.
//! Arbitrary error messages and source chains stay out of request logs.

use crate::domain::lettering::errors::DomainError;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::fmt;

/// Application-level errors returned from handlers.
///
/// Each variant maps to a status, a public message, and a fixed log category.
#[derive(Debug)]
pub enum AppError {
    Unauthorized,
    PayloadTooLarge(&'static str),
    /// A reviewed record changed before saving.
    Conflict(String),
    /// Resource not found (404).
    #[allow(dead_code)]
    NotFound(String),

    /// Request validation failed (400).
    #[allow(dead_code)]
    BadRequest(String),

    /// Access denied - authentication/authorization required (403).
    #[allow(dead_code)]
    Forbidden(String),

    /// Request data failed validation (400).
    #[allow(dead_code)]
    ValidationError(String),

    /// Rate limit exceeded (429).
    #[allow(dead_code)]
    RateLimited,

    /// Database operation failed (500).
    Database(String),

    /// Storage/file operation failed (500).
    Storage(String),

    /// ML model/detection failed (500).
    MlProcessing(String),

    /// Redis/queue operation failed (500).
    Queue(String),

    /// External service failure (503).
    #[allow(dead_code)]
    ExternalService(String),

    /// Unclassified internal error (500).
    #[allow(dead_code)]
    Internal(String),
}

impl std::error::Error for AppError {}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict(msg) => write!(f, "Conflict: {}", msg),
            Self::Unauthorized => write!(f, "Authentication required"),
            Self::PayloadTooLarge(message) => write!(f, "{message}"),
            Self::NotFound(msg) => write!(f, "Not found: {}", msg),
            Self::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            Self::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            Self::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            Self::RateLimited => write!(f, "Rate limit exceeded"),
            Self::Database(msg) => write!(f, "Database error: {}", msg),
            Self::Storage(msg) => write!(f, "Storage error: {}", msg),
            Self::MlProcessing(msg) => write!(f, "ML processing error: {}", msg),
            Self::Queue(msg) => write!(f, "Queue error: {}", msg),
            Self::ExternalService(msg) => write!(f, "External service error: {}", msg),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl AppError {
    /// Get the appropriate HTTP status code for this error.
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::ValidationError(_) => StatusCode::BAD_REQUEST,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Database(_) | Self::Storage(_) | Self::MlProcessing(_) | Self::Queue(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::ExternalService(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub(crate) fn category(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::PayloadTooLarge(_) => "payload_too_large",
            Self::Conflict(_) => "conflict",
            Self::NotFound(_) => "not_found",
            Self::BadRequest(_) => "bad_request",
            Self::Forbidden(_) => "forbidden",
            Self::ValidationError(_) => "validation",
            Self::RateLimited => "rate_limited",
            Self::Database(_) => "database",
            Self::Storage(_) => "storage",
            Self::MlProcessing(_) => "ml_processing",
            Self::Queue(_) => "queue",
            Self::ExternalService(_) => "external_service",
            Self::Internal(_) => "internal",
        }
    }

    /// Get a user-safe error message (without implementation details).
    pub(crate) fn user_message(&self) -> String {
        match self {
            Self::Unauthorized => "Authentication required".into(),
            Self::PayloadTooLarge(message) => (*message).into(),
            Self::Conflict(msg) => msg.clone(),
            Self::NotFound(_) => "Resource not found".into(),
            Self::BadRequest(msg) => msg.clone(),
            Self::Forbidden(_) => "Access denied".into(),
            Self::ValidationError(msg) => msg.clone(),
            Self::RateLimited => "Too many requests, please try again later".into(),
            Self::Database(_) => "Database operation failed".into(),
            Self::Storage(_) => "File operation failed".into(),
            Self::MlProcessing(_) => "Processing failed".into(),
            Self::Queue(_) => "Request queuing failed".into(),
            Self::ExternalService(_) => "External service unavailable".into(),
            Self::Internal(_) => "Internal server error".into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let message = self.user_message();

        match status {
            StatusCode::INTERNAL_SERVER_ERROR | StatusCode::SERVICE_UNAVAILABLE => {
                tracing::error!(
                    category = self.category(),
                    status = status.as_u16(),
                    "Request failed"
                );
            }
            StatusCode::BAD_REQUEST | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => {
                tracing::warn!(
                    category = self.category(),
                    status = status.as_u16(),
                    "Request failed"
                );
            }
            StatusCode::TOO_MANY_REQUESTS => {
                tracing::debug!(
                    category = self.category(),
                    status = status.as_u16(),
                    "Request failed"
                );
            }
            _ => {
                tracing::info!(
                    category = self.category(),
                    status = status.as_u16(),
                    "Request failed"
                );
            }
        }

        (status, Json(json!({ "error": message }))).into_response()
    }
}

// === Domain Error Conversion ===

impl From<DomainError> for AppError {
    fn from(err: DomainError) -> Self {
        match err {
            DomainError::NotFound(msg) => AppError::NotFound(msg),
            DomainError::ValidationError(msg) => AppError::ValidationError(msg),
            DomainError::InfrastructureError(msg) => AppError::Internal(msg),
            DomainError::RateLimitExceeded => AppError::RateLimited,
            DomainError::Unauthorized => AppError::Forbidden("Unauthorized".into()),
        }
    }
}

// === Database Error Conversion ===

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => AppError::NotFound("Record not found in database".into()),
            sqlx::Error::Configuration(_) => {
                AppError::Internal("Database configuration error".to_string())
            }
            sqlx::Error::Io(_) => AppError::Database("Database I/O error".to_string()),
            sqlx::Error::Tls(_) => AppError::Database("Database TLS error".to_string()),
            sqlx::Error::PoolTimedOut => AppError::Database("Connection pool exhausted".into()),
            sqlx::Error::PoolClosed => AppError::Database("Database connection unavailable".into()),
            sqlx::Error::Migrate(e) => AppError::Database(format!("Migration error: {}", e)),
            _ => AppError::Database("Database error".to_string()),
        }
    }
}

// === Redis/Queue Error Conversion ===

impl From<redis::RedisError> for AppError {
    fn from(err: redis::RedisError) -> Self {
        AppError::Queue(format!("Redis error: {}", err))
    }
}

// === HTTP Client Error Conversion ===

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            AppError::ExternalService("Request timeout".into())
        } else if err.is_connect() {
            AppError::ExternalService("Connection failed".into())
        } else if err.is_request() {
            AppError::BadRequest("Invalid request".into())
        } else if err.is_status() {
            AppError::ExternalService("External service error".into())
        } else {
            AppError::ExternalService("External service unavailable".into())
        }
    }
}

// === Image Processing Error Conversion ===

impl From<image::ImageError> for AppError {
    fn from(err: image::ImageError) -> Self {
        match err {
            image::ImageError::Unsupported(_) => {
                AppError::BadRequest("Unsupported image format".into())
            }
            image::ImageError::IoError(_) => AppError::Storage("Image reading failed".into()),
            image::ImageError::Decoding(_) => AppError::BadRequest("Invalid image data".into()),
            image::ImageError::Encoding(_) => AppError::Storage("Image encoding failed".into()),
            image::ImageError::Limits(_) => AppError::BadRequest("Image exceeds limits".into()),
            image::ImageError::Parameter(_) => {
                AppError::BadRequest("Invalid image operation".into())
            }
        }
    }
}

// === General Fallback Error Conversion ===

impl From<anyhow::Error> for AppError {
    fn from(_err: anyhow::Error) -> Self {
        AppError::Internal("Operation failed".into())
    }
}

impl From<crate::infrastructure::security::passwords::PasswordError> for AppError {
    fn from(error: crate::infrastructure::security::passwords::PasswordError) -> Self {
        use crate::infrastructure::security::passwords::PasswordError;
        match error {
            PasswordError::InvalidNewPassword | PasswordError::TooLong => {
                Self::BadRequest(error.to_string())
            }
            PasswordError::Work(_) => {
                Self::ExternalService("Password processing unavailable".into())
            }
            PasswordError::UnsupportedHash | PasswordError::Processing => {
                Self::Internal("Password processing failed".into())
            }
        }
    }
}

impl From<crate::infrastructure::images::ImageError> for AppError {
    fn from(error: crate::infrastructure::images::ImageError) -> Self {
        use crate::infrastructure::images::ImageError;
        match error {
            ImageError::CompressedLimit => Self::PayloadTooLarge("Image file is too large"),
            ImageError::DecodedLimit => {
                Self::PayloadTooLarge("Image resolution is too large; choose a smaller image")
            }
            ImageError::Invalid => Self::BadRequest(error.to_string()),
            ImageError::Encoding => Self::Storage(error.to_string()),
            ImageError::Work(_) => Self::ExternalService("Image processing unavailable".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(
            AppError::NotFound("test".into()).status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            AppError::BadRequest("test".into()).status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            AppError::RateLimited.status_code(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            AppError::Database("test".into()).status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_error_display() {
        let err = AppError::NotFound("item".into());
        assert_eq!(err.to_string(), "Not found: item");
    }
}
