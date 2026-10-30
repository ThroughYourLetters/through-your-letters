use axum::{
    Json,
    extract::{DefaultBodyLimit, FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;
use std::time::Duration;

/// Password endpoints accept at most 16 KiB and allow ten seconds to read JSON.
pub struct BoundedJson<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for BoundedJson<T> {
    type Rejection = Response;

    async fn from_request(mut request: Request, state: &S) -> Result<Self, Self::Rejection> {
        DefaultBodyLimit::max(16 * 1024).apply(&mut request);
        match tokio::time::timeout(
            Duration::from_secs(10),
            Json::<T>::from_request(request, state),
        )
        .await
        {
            Ok(Ok(Json(value))) => Ok(Self(value)),
            Ok(Err(error)) => Err(error.into_response()),
            Err(_) => Err((
                StatusCode::REQUEST_TIMEOUT,
                Json(serde_json::json!({
                    "error": "Request body timed out"
                })),
            )
                .into_response()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn slow_json_body_times_out_without_waiting_for_eof() {
        let body = axum::body::Body::from_stream(futures_util::stream::pending::<
            Result<bytes::Bytes, std::io::Error>,
        >());
        let request = Request::builder()
            .header("content-type", "application/json")
            .body(body)
            .unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(15),
            BoundedJson::<serde_json::Value>::from_request(request, &()),
        )
        .await
        .unwrap();
        assert_eq!(result.err().unwrap().status(), StatusCode::REQUEST_TIMEOUT);
    }
}
