use axum::http::{HeaderMap, header};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};

use crate::presentation::http::errors::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserClaims {
    pub sub: String,
    pub email: String,
    pub role: String,
    pub exp: usize,
    #[serde(default)]
    pub session_version: i64,
}

pub fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

pub fn decode_optional_user_claims(headers: &HeaderMap, secret: &str) -> Option<UserClaims> {
    let token = extract_bearer_token(headers)?;
    decode::<UserClaims>(
        &token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .ok()
    .map(|d| d.claims)
}

pub fn decode_required_user_claims(
    headers: &HeaderMap,
    secret: &str,
) -> Result<UserClaims, AppError> {
    decode_optional_user_claims(headers, secret).ok_or(AppError::Unauthorized)
}

/// Reject revoked sessions before handlers run.
pub async fn validate_session(
    axum::extract::State(state): axum::extract::State<crate::presentation::http::state::AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, AppError> {
    if !request.uri().path().starts_with("/api/v1/admin/")
        && request.headers().contains_key(header::AUTHORIZATION)
    {
        let claims = decode_required_user_claims(request.headers(), &state.config.jwt_secret)?;
        let id = uuid::Uuid::parse_str(&claims.sub).map_err(|_| AppError::Unauthorized)?;
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1 AND deleted_at IS NULL AND session_version = $2)",
        ).bind(id).bind(claims.session_version).fetch_one(&state.db).await
            .map_err(|e| AppError::Database(e.to_string()))?;
        if !valid {
            return Err(AppError::Unauthorized);
        }
    }
    if let Some(segment) = request
        .uri()
        .path()
        .strip_prefix("/api/v1/letterings/")
        .and_then(|p| p.split('/').next())
        && let Ok(id) = uuid::Uuid::parse_str(segment)
    {
        let owner = decode_optional_user_claims(request.headers(), &state.config.jwt_secret)
            .and_then(|c| uuid::Uuid::parse_str(&c.sub).ok());
        let visible: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM letterings l JOIN cities c ON c.id = l.city_id LEFT JOIN region_policies rp ON rp.country_code = c.country_code WHERE l.id = $1 AND (l.user_id = $2 OR (l.status = 'APPROVED' AND COALESCE(rp.discoverability_enabled, true))))",
        ).bind(id).bind(owner).fetch_one(&state.db).await
            .map_err(|e| AppError::Database(e.to_string()))?;
        if !visible {
            return Err(AppError::NotFound("Lettering not found".into()));
        }
    }
    Ok(next.run(request).await)
}
