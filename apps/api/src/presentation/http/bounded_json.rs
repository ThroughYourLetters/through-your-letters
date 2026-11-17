use crate::{limits::HttpLimits, presentation::http::state::AppState};
use axum::{
    Json,
    extract::{DefaultBodyLimit, FromRef, FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;
use std::time::Duration;

/// Apply the smaller body budget used by authentication and account actions.
pub struct BoundedJson<T>(pub T);

/// Apply the ordinary JSON size budget and the shared body-read deadline.
pub struct LimitedJson<T>(pub T);

impl FromRef<AppState> for HttpLimits {
    fn from_ref(state: &AppState) -> Self {
        state.config.limits.http.clone()
    }
}

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for BoundedJson<T>
where
    HttpLimits: FromRef<S>,
{
    type Rejection = Response;
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let limits = HttpLimits::from_ref(state);
        read_json(
            request,
            state,
            limits.bounded_json_max_bytes,
            limits.body_read_timeout_ms,
        )
        .await
        .map(Self)
        .map_err(IntoResponse::into_response)
    }
}

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for LimitedJson<T>
where
    HttpLimits: FromRef<S>,
{
    type Rejection = Response;
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let limits = HttpLimits::from_ref(state);
        read_json(
            request,
            state,
            limits.json_max_bytes,
            limits.body_read_timeout_ms,
        )
        .await
        .map(Self)
        .map_err(IntoResponse::into_response)
    }
}

enum JsonReadError {
    Rejected(Box<axum::extract::rejection::JsonRejection>),
    TimedOut,
}

impl IntoResponse for JsonReadError {
    fn into_response(self) -> Response {
        match self {
            Self::Rejected(error) => error.into_response(),
            Self::TimedOut => (
                StatusCode::REQUEST_TIMEOUT,
                Json(serde_json::json!({"error": "Request body timed out"})),
            )
                .into_response(),
        }
    }
}

async fn read_json<T: DeserializeOwned, S: Send + Sync>(
    mut request: Request,
    state: &S,
    max_bytes: usize,
    timeout_ms: u32,
) -> Result<T, JsonReadError> {
    DefaultBodyLimit::max(max_bytes).apply(&mut request);
    match tokio::time::timeout(
        Duration::from_millis(timeout_ms.into()),
        Json::<T>::from_request(request, state),
    )
    .await
    {
        Ok(Ok(Json(value))) => Ok(value),
        Ok(Err(error)) => Err(JsonReadError::Rejected(Box::new(error))),
        Err(_) => Err(JsonReadError::TimedOut),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn slow_json_body_times_out_without_waiting_for_eof() {
        for bounded in [true, false] {
            let body = axum::body::Body::from_stream(futures_util::stream::pending::<
                Result<bytes::Bytes, std::io::Error>,
            >());
            let request = Request::builder()
                .header("content-type", "application/json")
                .body(body)
                .unwrap();
            let mut limits = crate::limits::Limits::bundled().unwrap().http;
            limits.body_read_timeout_ms = 100;
            let response = tokio::time::timeout(Duration::from_secs(2), async {
                if bounded {
                    BoundedJson::<serde_json::Value>::from_request(request, &limits)
                        .await
                        .err()
                } else {
                    LimitedJson::<serde_json::Value>::from_request(request, &limits)
                        .await
                        .err()
                }
            })
            .await
            .unwrap()
            .unwrap();
            assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
        }
    }
}
