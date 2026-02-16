use crate::{
    domain::lettering::repository::LetteringRepository,
    infrastructure::queue::redis_queue::MlJob,
    presentation::http::{
        errors::AppError, middleware::user::decode_optional_user_claims, state::AppState,
    },
};
use axum::{
    Json,
    extract::{Multipart, State},
    http::HeaderMap,
};
use image::{ImageFormat, imageops::FilterType};
use sha2::{Digest, Sha256};
use sqlx::types::ipnetwork::IpNetwork;
use std::{io::Cursor, str::FromStr};
use uuid::Uuid;

fn extract_client_ip(headers: &HeaderMap) -> Option<IpNetwork> {
    let raw = headers
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
        });

    raw.and_then(|ip| IpNetwork::from_str(ip).ok())
}

async fn approve_without_ml(
    state: &AppState,
    lettering_id: Uuid,
    fallback_text: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE letterings SET detected_text = $1, status = 'APPROVED', updated_at = NOW() WHERE id = $2",
    )
    .bind(fallback_text)
    .bind(lettering_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(format!("Auto-approval failed: {}", e)))?;

    let _ = state
        .ws_broadcaster
        .send(serde_json::json!({ "type": "PROCESSED", "id": lettering_id }).to_string());
    Ok(())
}

pub async fn upload_lettering(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut image_data = None;
    let mut contributor = String::new();
    let mut pin = String::new();
    let mut desc = None;
    let mut city_id = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::BadRequest("Field error".into()))?
    {
        match field.name().unwrap_or("") {
            "image" => {
                image_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|_| AppError::BadRequest("Byte error".into()))?,
                )
            }
            "contributor_tag" => contributor = field.text().await.unwrap_or_default(),
            "pin_code" => pin = field.text().await.unwrap_or_default(),
            "description" => desc = Some(field.text().await.unwrap_or_default()),
            "city_id" => city_id = Some(field.text().await.unwrap_or_default()),
            _ => {}
        }
    }

    let contributor = contributor.trim().to_string();
    if contributor.is_empty() {
        return Err(AppError::BadRequest("Contributor tag required".into()));
    }

    let pin = pin.trim().to_string();
    if pin.len() != 6 || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::BadRequest("pin_code must be 6 digits".into()));
    }

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
        .ok_or_else(|| AppError::BadRequest("city_id is required and must be a valid UUID".into()))?;

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
    let img = image::load_from_memory(&data)
        .map_err(|_| AppError::BadRequest("Invalid image format".into()))?;

    // Process Original
    let mut buf = Cursor::new(Vec::new());
    img.resize(1200, 1200, FilterType::Lanczos3)
        .write_to(&mut buf, ImageFormat::WebP)
        .map_err(|e| AppError::Internal(format!("Failed to encode image to WebP: {}", e)))?;

    let image_bytes = buf.into_inner();

    // Hash Check for Duplicates
    let mut hasher = Sha256::new();
    hasher.update(&image_bytes);
    let image_hash = format!("{:x}", hasher.finalize());

    if state
        .lettering_repo
        .find_by_image_hash(&image_hash)
        .await?
        .is_some()
    {
        return Err(AppError::BadRequest(
            "This exact image has already been archived".into(),
        ));
    }

    let image_url = state
        .storage
        .upload(
            &format!("letterings/{}.webp", id),
            image_bytes,
            "image/webp",
        )
        .await?;

    // Generate Thumbnail
    let mut thumb_buf = Cursor::new(Vec::new());
    img.thumbnail(400, 400)
        .write_to(&mut thumb_buf, ImageFormat::WebP)
        .map_err(|e| AppError::Internal(format!("Failed to encode thumbnail to WebP: {}", e)))?;

    let thumb_url = state
        .storage
        .upload(
            &format!("thumbs/{}.webp", id),
            thumb_buf.into_inner(),
            "image/webp",
        )
        .await?;

    // let (mut lng, mut lat) = crate::infrastructure::geocoding::coordinates_for_pincode(&pin);
    // if (lng - 77.5946).abs() < 0.0001 && (lat - 12.9716).abs() < 0.0001 {
    //     let city_row = sqlx::query!("SELECT center_lat, center_lng FROM cities WHERE id = $1", city_id)
    //         .fetch_optional(&state.db)
    //         .await
    //         .unwrap_or(None);
    
    //     if let Some(row) = city_row {
    //         if let (Some(c_lat), Some(c_lng)) = (row.center_lat, row.center_lng) {
    //             lat = c_lat;
    //             lng = c_lng;
    //         }
    //     }
    // }
    // Fetch city coordinates for geolocation
    let city_coords = sqlx::query_as::<_, (f64, f64)>(
            "SELECT center_lng, center_lat FROM cities WHERE id = $1"
        )
        .bind(city_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error fetching city: {}", e);
            AppError::Internal(format!("Failed to fetch city coordinates: {}", e))
        })?;
    let final_lng = city_coords.0; 
    let final_lat = city_coords.1;

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
            location: crate::domain::lettering::entity::Coordinates {
                r#type: "Point".into(),
                coordinates: vec![final_lng, final_lat],
            },
            pin_code: pin,
            description: desc,
            image_hash: Some(image_hash),
            uploaded_by_ip: extract_client_ip(&headers),
            ..Default::default()
        };

    state.lettering_repo.create(&lettering).await?;

    // Attach user ownership if authenticated
    if let Some(claims) = decode_optional_user_claims(&headers, &state.config.jwt_secret) {
        if let Ok(user_id) = Uuid::parse_str(&claims.sub) {
            sqlx::query("UPDATE letterings SET user_id = $1 WHERE id = $2")
                .bind(user_id)
                .bind(id)
                .execute(&state.db)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to attach user ownership for lettering {}: {}", id, e);
                    AppError::Internal("Failed to link user ownership".into())
                })?;
        }
    }

    if state.config.enable_ml_processing {
        if let Err(err) = state
            .queue
            .enqueue_ml_job(MlJob {
                lettering_id: id,
                image_url,
            })
            .await
        {
            tracing::warn!("ML queue enqueue failed for {}: {}", id, err);
            // Fallback: approve without ML processing with empty detected text
            approve_without_ml(&state, id, "").await?;
            return Ok(Json(serde_json::json!({ "id": id, "status": "approved", "message": "Uploaded successfully but ML processing unavailable" })));
        }
    } else {
        // ML processing is disabled - approve immediately with empty detected text
        approve_without_ml(&state, id, "").await?;
        return Ok(Json(serde_json::json!({ "id": id, "status": "approved", "message": "Uploaded successfully (ML processing disabled)" })));
    }

    Ok(Json(
        serde_json::json!({ "id": id, "status": "processing" }),
    ))
}
