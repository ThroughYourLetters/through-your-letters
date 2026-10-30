use axum::{
    Json,
    extract::{Extension, Path, State},
    http::HeaderMap,
};
use chrono::{DateTime, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use ts_rs::TS;
use uuid::Uuid;

use crate::presentation::http::{
    bounded_json::BoundedJson,
    errors::AppError,
    middleware::{
        client_ip::ClientIp,
        rate_limit::enforce,
        user::{UserClaims, decode_required_user_claims},
    },
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, TS)]
pub struct AuthResponse {
    pub token: String,
    pub user: AuthUser,
}

#[derive(Debug, Serialize, FromRow, TS)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    password_hash: String,
    display_name: Option<String>,
    role: String,
    created_at: DateTime<Utc>,
}

async fn issue_user_token(state: &AppState, user: &AuthUser) -> Result<String, AppError> {
    let exp = (chrono::Utc::now() + chrono::Duration::days(7)).timestamp() as usize;
    let session_version: i64 = sqlx::query_scalar(
        "SELECT session_version FROM users WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;
    let claims = UserClaims {
        sub: user.id.to_string(),
        email: user.email.clone(),
        role: user.role.clone(),
        exp,
        session_version,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("Token generation failed: {}", e)))
}

pub async fn register(
    State(state): State<AppState>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    BoundedJson(body): BoundedJson<RegisterRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    enforce(
        &state,
        &format!("register:{ip}"),
        &state.config.limits.security.register,
    )
    .await?;

    let email = body.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(AppError::BadRequest("Valid email is required".to_string()));
    }

    let password_hash = state.passwords.hash(body.password).await?;

    // Generate email verification token
    let raw_verification_token = Uuid::now_v7().to_string() + &Uuid::now_v7().to_string();
    let verification_token_hash =
        format!("{:x}", Sha256::digest(raw_verification_token.as_bytes()));
    let verification_expires_at = chrono::Utc::now() + chrono::Duration::hours(24);

    let id = Uuid::now_v7();
    let insert_result = sqlx::query(
        "INSERT INTO users (id, email, password_hash, display_name, role, email_verification_token_hash, email_verification_expires_at) VALUES ($1, $2, $3, $4, 'USER', $5, $6)",
    )
    .bind(id)
    .bind(&email)
    .bind(&password_hash)
    .bind(body.display_name.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(&verification_token_hash)
    .bind(verification_expires_at)
    .execute(&state.db)
    .await;

    if let Err(e) = insert_result {
        if let sqlx::Error::Database(db_err) = &e
            && db_err.code().as_deref() == Some("23505")
        {
            return Err(AppError::BadRequest("Email already registered".to_string()));
        }
        return Err(AppError::Internal(e.to_string()));
    }

    // Delivery failures must never expose credentials.
    if let Err(e) = state
        .email_service
        .send_email_verification(&email, &raw_verification_token)
        .await
    {
        tracing::warn!(
            user_id = %id,
            error = %e,
            "Failed to send verification email"
        );
    }

    let user = AuthUser {
        id,
        email,
        display_name: body.display_name,
        role: "USER".to_string(),
        created_at: Utc::now(),
    };
    let token = issue_user_token(&state, &user).await?;

    Ok(Json(AuthResponse { token, user }))
}

pub async fn login_user(
    State(state): State<AppState>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    BoundedJson(body): BoundedJson<LoginRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    enforce(
        &state,
        &format!("login:{ip}"),
        &state.config.limits.security.login,
    )
    .await?;

    let email = body.email.trim().to_lowercase();
    if email.is_empty() {
        return Err(AppError::BadRequest("Email is required".to_string()));
    }

    let row = sqlx::query_as::<_, UserRow>(
        "SELECT id, email, password_hash, display_name, role, created_at FROM users WHERE email = $1 AND deleted_at IS NULL",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
    .ok_or(AppError::Unauthorized)?;

    let valid = state
        .passwords
        .verify(body.password, row.password_hash)
        .await?;

    if !valid {
        return Err(AppError::Unauthorized);
    }

    let user = AuthUser {
        id: row.id,
        email: row.email,
        display_name: row.display_name,
        role: row.role,
        created_at: row.created_at,
    };
    let token = issue_user_token(&state, &user).await?;

    Ok(Json(AuthResponse { token, user }))
}

pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthUser>, AppError> {
    let claims = decode_required_user_claims(&headers, &state.config.jwt_secret)?;
    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Forbidden("Invalid token subject".to_string()))?;

    let user = sqlx::query_as::<_, AuthUser>(
        "SELECT id, email, display_name, role, created_at FROM users WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
    .ok_or_else(|| AppError::Forbidden("User not found".to_string()))?;

    Ok(Json(user))
}

// ── Password reset ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

/// Request a password reset token. Always returns 200 to prevent email enumeration.
/// Tokens are delivered only through email.
pub async fn forgot_password(
    State(state): State<AppState>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    Json(body): Json<ForgotPasswordRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    enforce(
        &state,
        &format!("forgot:{ip}"),
        &state.config.limits.security.forgot_password,
    )
    .await?;

    let email = body.email.trim().to_lowercase();
    // Look up user — don't reveal whether the email exists
    let user_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE email = $1 AND deleted_at IS NULL")
            .bind(&email)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
            .flatten();

    if let Some(user_id) = user_id {
        // Invalidate any existing unused tokens for this user
        sqlx::query("UPDATE password_reset_tokens SET used_at = NOW() WHERE user_id = $1 AND used_at IS NULL")
            .bind(user_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Generate a cryptographically random token
        let raw_token = Uuid::now_v7().to_string() + &Uuid::now_v7().to_string();
        let token_hash = format!("{:x}", Sha256::digest(raw_token.as_bytes()));
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);

        sqlx::query(
            "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(Uuid::now_v7())
        .bind(user_id)
        .bind(&token_hash)
        .bind(expires_at)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        // Delivery failures must never expose credentials.
        if let Err(e) = state
            .email_service
            .send_password_reset(&email, &raw_token)
            .await
        {
            tracing::warn!(
                user_id = %user_id,
                error = %e,
                "Failed to send password reset email"
            );
        }
    }

    // Always return 200 to prevent email enumeration
    Ok(Json(serde_json::json!({
        "message": "If an account with that email exists, a password reset link has been sent."
    })))
}

/// Reset password using a valid token received via email.
pub async fn reset_password(
    State(state): State<AppState>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    BoundedJson(body): BoundedJson<ResetPasswordRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    enforce(
        &state,
        &format!("reset:{ip}"),
        &state.config.limits.security.reset_password,
    )
    .await?;

    let token_hash = format!("{:x}", Sha256::digest(body.token.as_bytes()));

    let password_hash = state.passwords.hash(body.new_password).await?;

    // Mark token as used and update password atomically
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Lock accounts before their recovery tokens.
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM users WHERE id = (SELECT user_id FROM password_reset_tokens WHERE token_hash = $1) AND deleted_at IS NULL FOR UPDATE",
    ).bind(&token_hash).fetch_optional(&mut *tx).await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::BadRequest("Invalid or expired reset token".into()))?;

    // Claim once inside the password transaction.
    let user_id: Uuid = sqlx::query_scalar(
        "UPDATE password_reset_tokens SET used_at = NOW() WHERE token_hash = $1 AND used_at IS NULL AND expires_at > NOW() RETURNING user_id",
    ).bind(&token_hash).fetch_optional(&mut *tx).await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::BadRequest("Invalid or expired reset token".into()))?;

    sqlx::query("UPDATE users SET password_hash = $1, session_version = session_version + 1, updated_at = NOW() WHERE id = $2 AND deleted_at IS NULL")
        .bind(&password_hash)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    tracing::info!(user_id = %user_id, "Password reset successfully");
    Ok(Json(
        serde_json::json!({ "message": "Password updated successfully." }),
    ))
}

// ── Email verification ────────────────────────────────────────────────────────

/// Verify an email address using the token sent at registration.
pub async fn verify_email(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));

    let row: Option<(Uuid, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
        "SELECT id, email_verification_expires_at FROM users WHERE email_verification_token_hash = $1 AND deleted_at IS NULL",
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let (user_id, expires_at) =
        row.ok_or_else(|| AppError::BadRequest("Invalid verification token".to_string()))?;

    if let Some(exp) = expires_at
        && chrono::Utc::now() > exp
    {
        return Err(AppError::BadRequest(
            "This verification token has expired".to_string(),
        ));
    }

    sqlx::query(
        "UPDATE users SET email_verified = true, email_verification_token_hash = NULL, email_verification_expires_at = NULL, updated_at = NOW() WHERE id = $1",
    )
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    tracing::info!(user_id = %user_id, "Email verified");
    Ok(Json(
        serde_json::json!({ "message": "Email verified successfully." }),
    ))
}
