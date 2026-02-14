use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};
use uuid::Uuid;

pub async fn request_id_middleware(req: Request, next: Next) -> Response {
    let request_id = Uuid::now_v7().to_string();

    let span = tracing::info_span!("request", id = %request_id);
    let _guard = span.enter();

    let mut response = next.run(req).await;
    if let Ok(val) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", val);
    }
    response
}
