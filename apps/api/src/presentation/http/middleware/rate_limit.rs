use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use redis::AsyncCommands;

use crate::presentation::http::state::AppState;

fn extract_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or("127.0.0.1")
        .to_string()
}

pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let ip = extract_client_ip(request.headers());
    if state.config.rate_limit_uploads_per_ip == 0 || ip == "127.0.0.1" || ip == "::1" {
        return Ok(next.run(request).await);
    }
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let key = format!("rate_limit:{}:{}", ip, date);

    let mut conn = state
        .redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let count: u32 = conn
        .incr(&key, 1_u32)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if count == 1 {
        let _: () = conn
            .expire(&key, 86_400)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    if count > state.config.rate_limit_uploads_per_ip {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(request).await)
}
