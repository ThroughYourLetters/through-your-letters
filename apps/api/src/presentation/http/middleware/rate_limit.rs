use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::presentation::http::state::AppState;

/// Extract client IP from headers. Does NOT special-case loopback addresses —
/// callers must never bypass rate limiting based on IP value, since X-Forwarded-For
/// can be spoofed by clients.
pub fn extract_client_ip(headers: &HeaderMap) -> String {
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
        .unwrap_or("unknown")
        .to_string()
}

/// Atomically increment a Redis counter and set its TTL on first creation.
/// Uses SET NX to initialize then INCR to avoid a TOCTOU race between INCR and EXPIRE.
pub async fn redis_incr_with_ttl(
    conn: &mut redis::aio::MultiplexedConnection,
    key: &str,
    ttl_secs: usize,
) -> Result<u64, redis::RedisError> {
    // Use a Lua script for atomic check-and-set + increment
    let script = redis::Script::new(
        r#"
        local current = redis.call('INCR', KEYS[1])
        if current == 1 then
            redis.call('EXPIRE', KEYS[1], ARGV[1])
        end
        return current
        "#,
    );
    script.key(key).arg(ttl_secs).invoke_async(conn).await
}

pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if state.config.rate_limit_uploads_per_ip == 0 {
        return Ok(next.run(request).await);
    }

    let ip = extract_client_ip(request.headers());
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let key = format!("upload_rate:{}:{}", ip, date);

    let mut conn = state
        .redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let count = redis_incr_with_ttl(&mut conn, &key, 86_400)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if count > state.config.rate_limit_uploads_per_ip as u64 {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(request).await)
}
