use crate::{infrastructure::images::ImageProcessor, presentation::http::errors::AppError};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{sync::Arc, time::Duration};

/// Hold an upload slot through body reading, processing, storage, and database writes.
pub async fn upload_budget(
    State(images): State<Arc<ImageProcessor>>,
    request: Request,
    next: Next,
) -> Response {
    let Ok(_permit) = images.admit_upload() else {
        return AppError::ExternalService("Upload capacity unavailable".into()).into_response();
    };
    match tokio::time::timeout(
        Duration::from_millis(images.limits().upload_timeout_ms),
        next.run(request),
    )
    .await
    {
        Ok(response) => response,
        Err(_) => AppError::ExternalService("Upload deadline exceeded".into()).into_response(),
    }
}
