use crate::infrastructure::monitoring::HttpMetrics;
use axum::{
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::Response,
};
use std::{sync::Arc, time::Instant};

pub async fn logging_middleware(
    State(metrics): State<Arc<HttpMetrics>>,
    request: Request,
    next: Next,
) -> Response {
    // MatchedPath contains route templates, never IDs or query strings.
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".into());
    let method = HttpMetrics::method(request.method());
    let started = Instant::now();
    let response = next.run(request).await;
    let elapsed = started.elapsed();
    metrics.record(&route, method, response.status(), elapsed);
    tracing::info!(
        route,
        method,
        status = response.status().as_u16(),
        elapsed_ms = elapsed.as_secs_f64() * 1000.0,
        "HTTP response completed"
    );
    response
}
