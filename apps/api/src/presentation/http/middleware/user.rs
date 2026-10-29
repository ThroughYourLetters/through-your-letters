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
        && let Ok(id) = uuid::Uuid::parse_str(
            &percent_encoding::percent_decode_str(segment)
                .decode_utf8()
                .map_err(|_| AppError::BadRequest("Invalid upload ID".into()))?,
        )
    {
        let owner = decode_optional_user_claims(request.headers(), &state.config.jwt_secret)
            .and_then(|c| uuid::Uuid::parse_str(&c.sub).ok());
        let visible: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM readable_letterings($2) WHERE id = $1)",
        )
        .bind(id)
        .bind(owner)
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
        if !visible {
            return Err(AppError::NotFound("Lettering not found".into()));
        }
    }
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An independently generated HS256 fixture must survive crypto-provider updates.
    #[test]
    fn existing_hmac_tokens_verify_and_tampered_tokens_are_rejected() {
        // Public test fixture generated with Python's hmac/SHA-256 implementation.
        const TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJsZWdhY3ktdXNlciIsImVtYWlsIjoibGVnYWN5QGV4YW1wbGUudGVzdCIsInJvbGUiOiJVU0VSIiwiZXhwIjo0MDAwMDAwMDAwLCJzZXNzaW9uX3ZlcnNpb24iOjd9.57A-qSb6FFPWT2hmfUXl3P_cl6gOtKJLEK9scISk7y8";
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {TOKEN}").parse().unwrap(),
        );
        let claims = decode_required_user_claims(&headers, "legacy-test-secret").unwrap();
        assert_eq!(claims.sub, "legacy-user");
        assert_eq!(claims.session_version, 7);
        assert!(decode_required_user_claims(&headers, "another-key").is_err());

        let mut tampered = TOKEN.as_bytes().to_vec();
        let signature = TOKEN.rfind('.').unwrap() + 1;
        tampered[signature] = b'6';
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", String::from_utf8(tampered).unwrap())
                .parse()
                .unwrap(),
        );
        assert!(decode_required_user_claims(&headers, "legacy-test-secret").is_err());

        let (_, payload_and_signature) = TOKEN.split_once('.').unwrap();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.{payload_and_signature}")
                .parse()
                .unwrap(),
        );
        assert!(decode_required_user_claims(&headers, "legacy-test-secret").is_err());
    }
}
