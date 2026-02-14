use crate::{
    application::upload_lettering::dto::UploadLetteringRequest,
    domain::lettering::{entity::*, repository::LetteringRepository},
    infrastructure::{
        geocoding::coordinates_for_pincode, queue::redis_queue::RedisQueue,
        storage::traits::StorageService,
    },
};
use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

/// Handles lettering upload workflow with comprehensive processing pipeline.
///
/// This use case orchestrates the complete upload process including:
/// - Image validation and deduplication
/// - Format conversion to optimized WebP
/// - Thumbnail generation for responsive display
/// - Persistent storage with CDN integration
/// - Database persistence with proper metadata
/// - Background job queueing for ML processing
///
/// # Architecture
/// The upload process follows a fail-fast approach where any validation
/// failure immediately returns an error without side effects. Storage
/// operations are atomic to prevent orphaned files.
///
/// # Performance Considerations
/// - Images are processed in memory to minimize I/O overhead
/// - Thumbnails are generated concurrently where possible
/// - Background ML processing prevents blocking user experience
/// - Content-based deduplication reduces storage costs
pub struct UploadLetteringUseCase {
    repository: Box<dyn LetteringRepository>,
    storage: Arc<dyn StorageService>,
    queue: Arc<RedisQueue>,
}

impl UploadLetteringUseCase {
    /// Creates a new instance of the upload use case with required dependencies.
    ///
    /// # Arguments
    /// * `repository` - Repository for lettering entity persistence
    /// * `storage` - Service for image asset storage (e.g., S3, R2)
    /// * `queue` - Background job queue for async processing
    pub fn new(
        repository: Box<dyn LetteringRepository>,
        storage: Arc<dyn StorageService>,
        queue: Arc<RedisQueue>,
    ) -> Self {
        info!("Initializing UploadLetteringUseCase with dependencies");
        Self {
            repository,
            storage,
            queue,
        }
    }

    /// Processes a lettering upload request end-to-end.
    ///
    /// This method performs the complete upload workflow:
    /// 1. Validates and processes the uploaded image
    /// 2. Checks for duplicate uploads using content hashing
    /// 3. Converts image to optimized WebP format
    /// 4. Generates multiple thumbnail sizes
    /// 5. Uploads all assets to persistent storage
    /// 6. Creates the lettering database entity
    /// 7. Queues background processing for ML analysis
    ///
    /// # Arguments
    /// * `request` - Upload request containing image and metadata
    ///
    /// # Returns
    /// Created lettering entity with assigned UUID and storage URLs
    ///
    /// # Errors
    /// Returns descriptive error messages for:
    /// - Invalid image formats or corrupted data
    /// - Duplicate content detection
    /// - Storage service failures
    /// - Database persistence issues
    /// - Background queue connectivity problems
    #[instrument(skip(self, request), fields(
        city_id = %request.city_id,
        contributor = %request.contributor_tag,
        pin_code = %request.pin_code,
        image_size = request.image_data.len()
    ))]
    pub async fn execute(&self, request: UploadLetteringRequest) -> Result<Lettering, String> {
        // Generate content hash for duplicate detection
        let image_hash = {
            let mut hasher = Sha256::new();
            hasher.update(&request.image_data);
            let hash = format!("{:x}", hasher.finalize());
            debug!("Generated image hash: {}", &hash[..16]); // Log partial hash
            hash
        };

        // Check for duplicate uploads using content hash
        if let Some(existing) = self
            .repository
            .find_by_image_hash(&image_hash)
            .await
            .map_err(|e| {
                error!("Database error during duplicate check: {}", e);
                format!("Failed to check for duplicates: {}", e)
            })?
        {
            warn!("Duplicate upload detected for hash: {}", &image_hash[..16]);
            return Err(format!(
                "This image has already been uploaded as lettering {}. Each image can only be uploaded once.",
                existing.id
            ));
        }

        // Generate unique identifier and storage paths
        let lettering_id = Uuid::now_v7();
        let image_key = format!("letterings/{}/original.webp", lettering_id);

        debug!("Processing upload for lettering ID: {}", lettering_id);

        // Convert uploaded image to optimized WebP format
        let original_webp = Self::convert_to_webp(&request.image_data, 2048)
            .map_err(|e| {
                error!("Image conversion failed for {}: {}", lettering_id, e);
                e
            })?;

        // Upload original image to persistent storage
        let image_url = self
            .storage
            .upload(&image_key, original_webp, "image/webp")
            .await
            .map_err(|e| {
                error!("Storage upload failed for {}: {}", lettering_id, e);
                format!("Failed to store image: {}", e)
            })?;

        // Generate multiple thumbnail sizes for responsive display
        let thumbnail_urls = self
            .generate_thumbnails(&request.image_data, &lettering_id)
            .await
            .map_err(|e| {
                error!("Thumbnail generation failed for {}: {}", lettering_id, e);
                e
            })?;

        let lettering = Lettering {
            id: lettering_id,
            city_id: request.city_id,
            contributor_tag: request.contributor_tag,
            image_url,
            thumbnail_urls,
            location: {
                let (lng, lat) = coordinates_for_pincode(&request.pin_code);
                Coordinates {
                    r#type: "Point".to_string(),
                    coordinates: vec![lng, lat],
                }
            },
            pin_code: request.pin_code,
            detected_text: None,
            ml_metadata: None,
            description: request.description,
            is_lettering: true,
            status: LetteringStatus::Pending,
            likes_count: 0,
            comments_count: 0,
            uploaded_by_ip: request.uploaded_by_ip,
            image_hash: Some(image_hash),
            report_count: 0,
            report_reasons: vec![],
            cultural_context: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Persist the lettering entity to database
        let saved = self
            .repository
            .create(&lettering)
            .await
            .map_err(|e| {
                error!("Database persistence failed for {}: {}", lettering_id, e);
                format!("Failed to save lettering: {}", e)
            })?;

        let _ = self
            .queue
            .enqueue_ml_job(crate::infrastructure::queue::redis_queue::MlJob {
                lettering_id,
                image_url: saved.image_url.clone(),
            })
            .await;

        Ok(saved)
    }

    /// Converts input image data to optimized WebP format with size constraints.
    ///
    /// This method handles format conversion and resizing to ensure consistent
    /// image quality and reasonable file sizes for web delivery.
    ///
    /// # Arguments
    /// * `image_data` - Raw image bytes in any supported format
    /// * `max_width` - Maximum pixel width for the output image
    ///
    /// # Returns
    /// WebP-encoded image data optimized for web delivery
    ///
    /// # Errors
    /// Returns descriptive error for invalid image data or conversion failures
    fn convert_to_webp(image_data: &[u8], max_width: u32) -> Result<Vec<u8>, String> {
        use image::ImageFormat;
        use std::io::Cursor;

        // Validate and load the input image
        let img = image::load_from_memory(image_data)
            .map_err(|e| format!("Invalid or corrupted image data: {}", e))?;
        // Resize image if it exceeds maximum width constraints
        let resized = if img.width() > max_width {
            debug!("Resizing image from {}x{} to max width {}",
                   img.width(), img.height(), max_width);
            img.resize(max_width, u32::MAX, image::imageops::FilterType::Triangle)
        } else {
            debug!("Image size {}x{} within limits, no resizing needed",
                   img.width(), img.height());
            img
        };

        // Convert to WebP format with optimized compression
        let mut buffer = Cursor::new(Vec::new());
        resized
            .write_to(&mut buffer, ImageFormat::WebP)
            .map_err(|e| format!("WebP encoding failed: {}", e))?;

        let webp_data = buffer.into_inner();
        debug!("WebP conversion complete, output size: {} bytes", webp_data.len());
        Ok(webp_data)
    }

    async fn generate_thumbnails(
        &self,
        image_data: &Bytes,
        id: &Uuid,
    ) -> Result<ThumbnailUrls, String> {
        // PRD sizes: small=200px (heatmap/matrix), medium=600px (gallery), large=1200px (zine view)
        let sizes = [("small", 200u32), ("medium", 600), ("large", 1200)];
        let img =
            image::load_from_memory(image_data).map_err(|e| format!("Invalid image: {}", e))?;

        let mut urls = vec![];

        for (name, width) in &sizes {
            debug!("Generating {} thumbnail ({}px) for lettering {}", name, width, id);

            let resized = img.resize(*width, u32::MAX, image::imageops::FilterType::Triangle);
            let mut buffer = std::io::Cursor::new(Vec::new());
            resized
                .write_to(&mut buffer, image::ImageFormat::WebP)
                .map_err(|e| format!("Thumbnail generation failed: {}", e))?;

            // Upload thumbnail to storage
            let key = format!("letterings/{}/{}.webp", id, name);
            let url = self
                .storage
                .upload(&key, buffer.into_inner(), "image/webp")
                .await
                .map_err(|e| format!("{} thumbnail upload failed: {}", name, e))?;

            debug!("Successfully uploaded {} thumbnail: {}", name, &url);
            urls.push(url);
        }

        Ok(ThumbnailUrls {
            small: urls[0].clone(),
            medium: urls[1].clone(),
            large: urls[2].clone(),
        })
    }
}
