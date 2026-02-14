use axum::{Json, extract::State, http::HeaderMap};
use bcrypt::{DEFAULT_COST, hash, verify};
use chrono::{DateTime, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::presentation::http::{
    errors::AppError,
    middleware::user::{UserClaims, decode_required_user_claims},
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

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: AuthUser,
}

#[derive(Debug, Serialize, FromRow)]
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

fn issue_user_token(state: &AppState, user: &AuthUser) -> Result<String, AppError> {
    let exp = (chrono::Utc::now() + chrono::Duration::days(7)).timestamp() as usize;
    let claims = UserClaims {
        sub: user.id.to_string(),
        email: user.email.clone(),
        role: user.role.clone(),
        exp,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::InternalError(format!("Token generation failed: {}", e)))
}

pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(AppError::BadRequest("Valid email is required".to_string()));
    }
    if body.password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".to_string(),
        ));
    }

    let password_hash = hash(&body.password, DEFAULT_COST)
        .map_err(|e| AppError::InternalError(format!("Password hashing failed: {}", e)))?;

    let id = Uuid::now_v7();
    let insert_result = sqlx::query(
        "INSERT INTO users (id, email, password_hash, display_name, role) VALUES ($1, $2, $3, $4, 'USER')",
    )
    .bind(id)
    .bind(&email)
    .bind(&password_hash)
    .bind(body.display_name.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .execute(&state.db)
    .await;

    if let Err(e) = insert_result {
        if let sqlx::Error::Database(db_err) = &e {
            if db_err.code().as_deref() == Some("23505") {
                return Err(AppError::BadRequest("Email already registered".to_string()));
            }
        }
        return Err(AppError::InternalError(e.to_string()));
    }

    let user = AuthUser {
        id,
        email,
        display_name: body.display_name,
        role: "USER".to_string(),
        created_at: Utc::now(),
    };
    let token = issue_user_token(&state, &user)?;

    Ok(Json(AuthResponse { token, user }))
}

pub async fn login_user(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    let email = body.email.trim().to_lowercase();
    if email.is_empty() {
        return Err(AppError::BadRequest("Email is required".to_string()));
    }

    let row = sqlx::query_as::<_, UserRow>(
        "SELECT id, email, password_hash, display_name, role, created_at FROM users WHERE email = $1",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::InternalError(e.to_string()))?
    .ok_or_else(|| AppError::Forbidden("Invalid credentials".to_string()))?;

    let valid = verify(&body.password, &row.password_hash)
        .map_err(|_| AppError::InternalError("Password verification failed".to_string()))?;

    if !valid {
        return Err(AppError::Forbidden("Invalid credentials".to_string()));
    }

    let user = AuthUser {
        id: row.id,
        email: row.email,
        display_name: row.display_name,
        role: row.role,
        created_at: row.created_at,
    };
    let token = issue_user_token(&state, &user)?;

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
        "SELECT id, email, display_name, role, created_at FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::InternalError(e.to_string()))?
    .ok_or_else(|| AppError::Forbidden("User not found".to_string()))?;

    Ok(Json(user))
}
