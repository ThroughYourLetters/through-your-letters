use axum::{extract::Request, middleware::Next, response::Response};

pub async fn logging_middleware(request: Request, next: Next) -> Response {
    tracing::info!("Request: {} {}", request.method(), request.uri());
    next.run(request).await
}
