use crate::{
    domain::lettering::{
        entity::{Lettering, LetteringStatus},
        repository::{LetteringRepository, NewUpload, UploadRequest},
        value_objects::PinCode,
    },
    presentation::http::{
        errors::AppError,
        middleware::{client_ip::ClientIp, user::decode_optional_user_claims},
        state::AppState,
    },
};
use axum::{
    Json,
    extract::{Extension, Multipart, State},
    http::HeaderMap,
};
use bytes::Bytes;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::types::ipnetwork::IpNetwork;
use std::num::NonZeroU32;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum UploadStatus {
    Pending,
    Processing,
    Approved,
    Rejected,
    Reported,
}

#[derive(Debug, Serialize, TS)]
pub struct UploadResponse {
    pub id: Uuid,
    pub location_source: crate::domain::lettering::entity::LocationSource,
    pub location_warning: Option<String>,
    pub status: UploadStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
}

async fn approve_without_ml(
    state: &AppState,
    lettering_id: Uuid,
    fallback_text: &str,
) -> Result<String, AppError> {
    let approved = sqlx::query_scalar::<_, String>(
        "UPDATE letterings SET detected_text = $1, status = 'APPROVED', updated_at = NOW() WHERE id = $2 AND status = 'PENDING' AND moderated_at IS NULL RETURNING status",
    )
    .bind(fallback_text)
    .bind(lettering_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(format!("Auto-approval failed: {}", e)))?;

    if let Some(status) = approved {
        let _ = state
            .ws_broadcaster
            .send(serde_json::json!({ "type": "PROCESSED", "id": lettering_id }).to_string());
        return Ok(status.to_ascii_lowercase());
    }
    let status: String = sqlx::query_scalar("SELECT status FROM letterings WHERE id=$1")
        .bind(lettering_id)
        .fetch_one(&state.db)
        .await?;
    Ok(status.to_ascii_lowercase())
}

async fn read_field(
    field: &mut axum::extract::multipart::Field<'_>,
    max_bytes: usize,
) -> Result<Vec<u8>, AppError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
        if chunk.len() > max_bytes.saturating_sub(bytes.len()) {
            return Err(AppError::PayloadTooLarge(
                if field.name() == Some("image") {
                    "Image file is too large"
                } else {
                    "Upload text field is too large"
                },
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn multipart_error(error: axum::extract::multipart::MultipartError) -> AppError {
    if error.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
        AppError::PayloadTooLarge("Upload body is too large")
    } else {
        AppError::BadRequest("Invalid multipart body".into())
    }
}

async fn upload_response(
    state: &AppState,
    lettering: &Lettering,
    analysis_requested: bool,
) -> Result<Json<UploadResponse>, AppError> {
    let status = if !analysis_requested {
        match approve_without_ml(state, lettering.id, "").await?.as_str() {
            "pending" => UploadStatus::Pending,
            "approved" => UploadStatus::Approved,
            "rejected" => UploadStatus::Rejected,
            "reported" => UploadStatus::Reported,
            _ => return Err(AppError::Internal("Unknown upload status".into())),
        }
    } else {
        match lettering.status {
            LetteringStatus::Pending => UploadStatus::Processing,
            LetteringStatus::Approved => UploadStatus::Approved,
            LetteringStatus::Rejected => UploadStatus::Rejected,
            LetteringStatus::Reported => UploadStatus::Reported,
        }
    };
    let warning = lettering
        .location_error
        .as_ref()
        .map(|_| "Location could not be resolved. The upload will not appear on the map.".into());
    Ok(Json(UploadResponse {
        id: lettering.id,
        location_source: lettering.location_source,
        location_warning: warning,
        status,
        message: (!analysis_requested)
            .then(|| "Uploaded successfully (ML processing disabled)".into()),
    }))
}

async fn replay_upload(
    state: &AppState,
    owner: Option<Uuid>,
    key: Uuid,
    payload_hash: &str,
) -> Result<Option<Json<UploadResponse>>, AppError> {
    let Some(receipt) = state.lettering_repo.find_upload_request(owner, key).await? else {
        return Ok(None);
    };
    if receipt.payload_hash != payload_hash {
        return Err(AppError::Conflict(
            "This request key was already used for different upload content".into(),
        ));
    }
    let id = receipt.lettering_id.ok_or_else(|| {
        AppError::Conflict("This upload was deleted; use a new request key for a new upload".into())
    })?;
    let lettering = state.lettering_repo.find_by_id(id).await?.ok_or_else(|| {
        AppError::Conflict("This upload was deleted; use a new request key for a new upload".into())
    })?;
    upload_response(state, &lettering, receipt.analysis_requested)
        .await
        .map(Some)
}

pub async fn upload_lettering(
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, AppError> {
    if headers.get_all("idempotency-key").iter().count() > 1 {
        return Err(AppError::BadRequest(
            "Provide only one Idempotency-Key header".into(),
        ));
    }
    let request_key = headers
        .get("idempotency-key")
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|value| Uuid::parse_str(value).ok())
                .filter(|value| !value.is_nil())
                .ok_or_else(|| {
                    AppError::BadRequest("Idempotency-Key must be a nonzero UUID".into())
                })
        })
        .transpose()?;
    let owner_id = decode_optional_user_claims(&headers, &state.config.jwt_secret)
        .map(|claims| Uuid::parse_str(&claims.sub))
        .transpose()
        .map_err(|_| AppError::Unauthorized)?;
    let mut image_data = None;
    let mut contributor = String::new();
    let mut pin = String::new();
    let mut desc = None;
    let mut city_id = None;
    let limits = &state.config.limits.images;
    let mut fields = 0;

    while let Some(mut field) = multipart.next_field().await.map_err(multipart_error)? {
        fields += 1;
        if fields > limits.max_fields {
            return Err(AppError::PayloadTooLarge("Upload contains too many fields"));
        }
        let name = field.name().unwrap_or("").to_owned();
        if name == "image" {
            if image_data.is_some() {
                return Err(AppError::BadRequest("Only one image is allowed".into()));
            }
            image_data = Some(Bytes::from(
                read_field(&mut field, limits.max_input_bytes).await?,
            ));
            continue;
        }
        let text = String::from_utf8(read_field(&mut field, limits.max_field_bytes).await?)
            .map_err(|_| AppError::BadRequest("Multipart text must be UTF-8".into()))?;
        match name.as_str() {
            "contributor_tag" => contributor = text,
            "pin_code" => pin = text,
            "description" => desc = Some(text),
            "city_id" => city_id = Some(text),
            _ => {}
        }
    }

    let contributor = contributor.trim().to_string();
    if contributor.is_empty() {
        return Err(AppError::BadRequest("Contributor tag required".into()));
    }

    let pin = PinCode::new(pin)
        .map_err(|error| AppError::BadRequest(format!("Invalid postal code: {error}")))?
        .value;

    let desc = desc.and_then(|d| {
        let trimmed = d.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    let data = image_data.ok_or(AppError::BadRequest("Missing image".into()))?;

    let city_id = city_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| {
            AppError::BadRequest("city_id is required and must be a valid UUID".into())
        })?;

    let upload_allowed = sqlx::query_scalar::<_, Option<bool>>(
        "SELECT COALESCE(rp.uploads_enabled, true)
         FROM cities c
         LEFT JOIN region_policies rp ON rp.country_code = c.country_code
         WHERE c.id = $1",
    )
    .bind(city_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
    .flatten()
    .ok_or_else(|| AppError::BadRequest("City not found".to_string()))?;

    if !upload_allowed {
        return Err(AppError::Forbidden(
            "Uploads are disabled for this region".to_string(),
        ));
    }

    // Virus Scanning
    let is_safe = state
        .virus_scanner
        .scan(&data)
        .await
        .map_err(|e| AppError::Internal(format!("Scanner failure: {}", e)))?;

    if !is_safe {
        return Err(AppError::Forbidden(
            "Security threat detected in file".into(),
        ));
    }

    let id = Uuid::now_v7();
    // Complete both transforms before starting object storage writes.
    let prepared = state.images.prepare_upload(data).await?;
    let image_hash = prepared.hash;
    let payload_hash = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(&image_hash, &contributor, &pin, &desc, city_id))
                .map_err(|_| AppError::Internal("Upload fingerprint encoding failed".into()))?
        )
    );
    if let Some(key) = request_key
        && let Some(response) = replay_upload(&state, owner_id, key, &payload_hash).await?
    {
        return Ok(response);
    }

    if state
        .lettering_repo
        .find_by_image_hash(&image_hash)
        .await?
        .is_some()
    {
        if let Some(key) = request_key
            && let Some(response) = replay_upload(&state, owner_id, key, &payload_hash).await?
        {
            return Ok(response);
        }
        return Err(AppError::BadRequest(
            "This exact image has already been archived".into(),
        ));
    }

    let storage_attempt = state
        .upload_attempts
        .begin(
            id,
            &state.storage.get_url(&format!("letterings/{id}.webp")),
            &state.storage.get_url(&format!("thumbs/{id}.webp")),
        )
        .await?;
    let image_url = state
        .storage
        .upload(
            &format!("letterings/{}.webp", id),
            prepared.image,
            "image/webp",
        )
        .await?;

    let thumb_url = state
        .storage
        .upload(
            &format!("thumbs/{}.webp", id),
            prepared.thumbnail,
            "image/webp",
        )
        .await?;

    let country_code: String = sqlx::query_scalar("SELECT country_code FROM cities WHERE id = $1")
        .bind(city_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch city: {}", e)))?
        .ok_or_else(|| AppError::BadRequest("City not found".into()))?;

    use crate::domain::lettering::entity::LocationSource;
    let (location, location_source, location_error) =
        match state.geocoder.postal_code(&pin, &country_code).await {
            Ok(point) => (Some(point), LocationSource::PostalCode, None),
            Err(error) => {
                tracing::warn!(%id, "Upload location unresolved");
                (None, LocationSource::Unresolved, Some(error.to_string()))
            }
        };

    let lettering = crate::domain::lettering::entity::Lettering {
        id,
        city_id,
        contributor_tag: contributor,
        image_url: image_url.clone(),
        thumbnail_urls: crate::domain::lettering::entity::ThumbnailUrls {
            small: thumb_url.clone(),
            medium: thumb_url.clone(),
            large: image_url.clone(),
        },
        location,
        location_source,
        location_error: location_error.clone(),
        pin_code: pin,
        description: desc,
        image_hash: Some(image_hash),
        uploaded_by_ip: Some(IpNetwork::from(ip)),
        ..Default::default()
    };

    let analysis_attempts = if state.config.enable_ml_processing {
        Some(
            NonZeroU32::new(state.config.limits.queue.job_max_attempts)
                .ok_or_else(|| AppError::Internal("Invalid analysis retry budget".into()))?,
        )
    } else {
        None
    };
    let saved = state
        .lettering_repo
        .create(NewUpload {
            lettering: &lettering,
            owner_id,
            analysis_attempts,
            request: request_key.map(|key| UploadRequest {
                key,
                payload_hash: &payload_hash,
            }),
            storage_attempt: Some(storage_attempt),
        })
        .await;
    if let Err(error) = saved {
        // A conflicting write or lost COMMIT response can still mean the request succeeded.
        if let Some(key) = request_key
            && let Some(response) = replay_upload(&state, owner_id, key, &payload_hash).await?
        {
            return Ok(response);
        }
        return Err(error.into());
    }
    upload_response(&state, &lettering, analysis_attempts.is_some()).await
}
