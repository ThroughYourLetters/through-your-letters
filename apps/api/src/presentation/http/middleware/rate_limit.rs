use super::client_ip::ClientIp;
use crate::{
    limits::RateLimitPolicy,
    presentation::http::{errors::AppError, state::AppState},
};
use axum::{
    extract::{Extension, Request, State},
    middleware::Next,
    response::Response,
};

/// Apply the same fail-closed policy to every protected operation.
pub async fn enforce(
    state: &AppState,
    key: &str,
    policy: &RateLimitPolicy,
) -> Result<(), AppError> {
    match state.rate_limiter.check(key, policy).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(AppError::RateLimited),
        Err(error) => {
            // Provider messages can contain request data; log only the failure category.
            tracing::warn!(kind = ?error.kind(), "Abuse protection unavailable");
            Err(AppError::ExternalService(
                "Abuse protection unavailable".into(),
            ))
        }
    }
}

pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let policy = RateLimitPolicy {
        requests: state.config.rate_limit_uploads_per_ip,
        window_seconds: 86400,
    };
    enforce(&state, &format!("upload:{ip}"), &policy).await?;
    Ok(next.run(request).await)
}
