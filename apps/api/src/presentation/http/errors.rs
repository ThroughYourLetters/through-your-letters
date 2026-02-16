//! HTTP error handling and response conversion.
//!
//! This module provides structured error types that are mapped to appropriate HTTP status codes
//! and JSON responses. Errors preserve their source chain for comprehensive logging and debugging.
//!
//! # Error Hierarchy
//!
//! Application errors are categorized into distinct types that map cleanly to HTTP status codes.
//! The error source chain is preserved to enable detailed logging and observability.

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
/// Each variant maps to a specific HTTP status code and error category.
/// Error source chains are preserved for comprehensive debugging.
#[derive(Debug)]
pub enum AppError {
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

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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

    /// Get a user-safe error message (without implementation details).
    fn user_message(&self) -> String {
        match self {
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

        // Log the error with full context
        match status {
            StatusCode::INTERNAL_SERVER_ERROR | StatusCode::SERVICE_UNAVAILABLE => {
                tracing::error!("error={}", self);
            }
            StatusCode::BAD_REQUEST | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => {
                tracing::warn!("error={}", self);
            }
            StatusCode::TOO_MANY_REQUESTS => {
                tracing::debug!("error={}", self);
            }
            _ => {
                tracing::info!("error={}", self);
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
            DomainError::InfrastructureError(msg) => {
                tracing::error!(infrastructure_error = %msg);
                AppError::Internal(msg)
            }
            DomainError::RateLimitExceeded => AppError::RateLimited,
            DomainError::Unauthorized => AppError::Forbidden("Unauthorized".into()),
        }
    }
}

// === Database Error Conversion ===

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => {
                AppError::NotFound("Record not found in database".into())
            }
            sqlx::Error::Configuration(msg) => {
                tracing::error!(database_config_error = %msg);
                AppError::Internal(format!("Database configuration error"))
            }
            sqlx::Error::Io(e) => {
                tracing::error!(database_io_error = %e);
                AppError::Database(format!("Database I/O error"))
            }
            sqlx::Error::Tls(e) => {
                tracing::error!(database_tls_error = %e);
                AppError::Database(format!("Database TLS error"))
            }
            sqlx::Error::PoolTimedOut => {
                tracing::warn!("Database connection pool exhausted, timing out");
                AppError::Database("Connection pool exhausted".into())
            }
            sqlx::Error::PoolClosed => {
                tracing::error!("Database connection pool closed");
                AppError::Database("Database connection unavailable".into())
            }
            sqlx::Error::Migrate(e) => {
                tracing::error!(migration_error = %e);
                AppError::Database(format!("Migration error: {}", e))
            }
            _ => {
                tracing::error!(database_error = %err);
                AppError::Database(format!("Database error"))
            }
        }
    }
}

// === Redis/Queue Error Conversion ===

impl From<redis::RedisError> for AppError {
    fn from(err: redis::RedisError) -> Self {
        tracing::error!(redis_error = %err, "Redis operation failed");
        AppError::Queue(format!("Redis error: {}", err))
    }
}

// === HTTP Client Error Conversion ===

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            tracing::warn!(reqwest_timeout = %err);
            AppError::ExternalService("Request timeout".into())
        } else if err.is_connect() {
            tracing::warn!(reqwest_connect = %err);
            AppError::ExternalService("Connection failed".into())
        } else if err.is_request() {
            tracing::warn!(reqwest_request = %err);
            AppError::BadRequest("Invalid request".into())
        } else if err.is_status() {
            tracing::info!(reqwest_status = %err);
            AppError::ExternalService("External service error".into())
        } else {
            tracing::error!(reqwest_error = %err);
            AppError::ExternalService("External service unavailable".into())
        }
    }
}

// === Image Processing Error Conversion ===

impl From<image::ImageError> for AppError {
    fn from(err: image::ImageError) -> Self {
        match err {
            image::ImageError::Unsupported(_) => {
                tracing::warn!(image_format_error = %err);
                AppError::BadRequest("Unsupported image format".into())
            }
            image::ImageError::IoError(_) => {
                tracing::error!(image_io_error = %err);
                AppError::Storage("Image reading failed".into())
            }
            image::ImageError::Decoding(_) => {
                tracing::warn!(image_decode_error = %err);
                AppError::BadRequest("Invalid image data".into())
            }
            image::ImageError::Encoding(_) => {
                tracing::error!(image_encode_error = %err);
                AppError::Storage("Image encoding failed".into())
            }
            image::ImageError::Limits(_) => {
                tracing::warn!(image_limits_error = %err);
                AppError::BadRequest("Image exceeds limits".into())
            }
            image::ImageError::Parameter(_) => {
                tracing::warn!(image_parameter_error = %err);
                AppError::BadRequest("Invalid image operation".into())
            }
        }
    }
}

// === General Fallback Error Conversion ===

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!(anyhow_error = %err, "Unclassified error with chain");
        err.chain().for_each(|cause| {
            tracing::error!(cause = %cause, "Error source");
        });
        AppError::Internal("Operation failed".into())
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
        assert_eq!(AppError::RateLimited.status_code(), StatusCode::TOO_MANY_REQUESTS);
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

