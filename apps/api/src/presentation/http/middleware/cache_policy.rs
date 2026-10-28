use crate::infrastructure::cache::response_cache::ResponseCache;
use axum::{
    extract::{Request, State},
    http::{HeaderValue, Method, header},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

pub async fn cache_policy(
    State(cache): State<Arc<ResponseCache>>,
    request: Request,
    next: Next,
) -> Response {
    let is_api = request.uri().path().starts_with("/api/");
    let is_write = !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    let path = request.uri().path();
    let affects_gallery = path.starts_with("/api/v1/letterings/")
        || path.starts_with("/api/v1/admin/letterings/")
        || path.starts_with("/api/v1/admin/region-policies/")
        || path.starts_with("/api/v1/me/letterings/");
    let mut response = next.run(request).await;
    if is_write && affects_gallery && response.status().is_success() {
        cache.invalidate().await;
    }
    if is_api {
        // Public responses are cached explicitly in memory on the server.
        // Never persist account/admin data in browser or shared HTTP caches.
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::StatusCode,
        middleware,
        routing::{get, post},
    };
    use tower::ServiceExt;

    fn router(cache: Arc<ResponseCache>, write_status: StatusCode) -> Router {
        Router::new()
            .route(
                "/api/v1/me/letterings/test",
                post(move || async move { write_status }),
            )
            .route("/api/v1/auth/me", get(|| async { "private" }))
            .route("/health", get(|| async { "healthy" }))
            .layer(middleware::from_fn_with_state(cache, cache_policy))
    }

    #[tokio::test]
    async fn successful_mutations_invalidate_warm_gallery_data() {
        let cache = Arc::new(ResponseCache::new(
            crate::limits::Limits::bundled().unwrap().cache,
        ));
        cache
            .get_or_fetch("gallery", 15, || async { Ok(1_u32) })
            .await
            .unwrap();
        let response = router(cache.clone(), StatusCode::NO_CONTENT)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/me/letterings/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            cache
                .get_or_fetch("gallery", 15, || async { Ok(2_u32) })
                .await
                .unwrap(),
            2
        );
    }

    #[tokio::test]
    async fn failed_mutations_preserve_cached_data() {
        let cache = Arc::new(ResponseCache::new(
            crate::limits::Limits::bundled().unwrap().cache,
        ));
        cache
            .get_or_fetch("gallery", 15, || async { Ok(1_u32) })
            .await
            .unwrap();
        let response = router(cache.clone(), StatusCode::FORBIDDEN)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/me/letterings/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            cache
                .get_or_fetch("gallery", 15, || async { Ok(2_u32) })
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn api_responses_are_no_store_without_changing_health() {
        let app = router(
            Arc::new(ResponseCache::new(
                crate::limits::Limits::bundled().unwrap().cache,
            )),
            StatusCode::NO_CONTENT,
        );
        for path in ["/api/v1/auth/me", "/api/v1/missing"] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        }
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(!response.headers().contains_key(header::CACHE_CONTROL));
    }
}
