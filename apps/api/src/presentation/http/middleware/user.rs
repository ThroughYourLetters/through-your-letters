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
    decode_optional_user_claims(headers, secret)
        .ok_or_else(|| AppError::Forbidden("Unauthorized".to_string()))
}
