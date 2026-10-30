use super::blocking::{BlockingError, BlockingPool};
use crate::limits::ImageLimits;
use bytes::{Bytes, BytesMut};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, imageops::FilterType};
use sha2::{Digest, Sha256};
use std::{io::Cursor, sync::Arc, time::Duration};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("Image exceeds the compressed byte limit")]
    CompressedLimit,
    #[error("Image exceeds the decoded size limit")]
    DecodedLimit,
    #[error("Invalid or unsupported image")]
    Invalid,
    #[error("Image encoding failed")]
    Encoding,
    #[error(transparent)]
    Work(#[from] BlockingError),
}

pub struct PreparedUpload {
    pub image: Vec<u8>,
    pub thumbnail: Vec<u8>,
    pub hash: String,
}

/// One CPU budget shared by upload transforms and local model operations.
pub struct ImageProcessor {
    pool: BlockingPool,
    uploads: Arc<Semaphore>,
    limits: ImageLimits,
}

impl ImageProcessor {
    pub fn new(limits: ImageLimits) -> Self {
        Self {
            pool: BlockingPool::new(
                limits.workers,
                Duration::from_millis(limits.work_timeout_ms),
            ),
            uploads: Arc::new(Semaphore::new(limits.concurrent_uploads.get())),
            limits,
        }
    }

    pub fn limits(&self) -> &ImageLimits {
        &self.limits
    }

    pub fn admit_upload(&self) -> Result<OwnedSemaphorePermit, BlockingError> {
        self.uploads
            .clone()
            .try_acquire_owned()
            .map_err(|_| BlockingError::Busy)
    }

    pub async fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, BlockingError> {
        self.pool.run(work).await
    }

    pub async fn prepare_upload(
        self: &Arc<Self>,
        bytes: Bytes,
    ) -> Result<PreparedUpload, ImageError> {
        let processor = self.clone();
        self.run(move || {
            let original = processor.decode(&bytes)?;
            let encode = |image: DynamicImage| -> Result<Vec<u8>, ImageError> {
                let mut bytes = Cursor::new(Vec::new());
                image
                    .write_to(&mut bytes, ImageFormat::WebP)
                    .map_err(|_| ImageError::Encoding)?;
                Ok(bytes.into_inner())
            };
            let image = encode(original.resize(1200, 1200, FilterType::Lanczos3))?;
            let thumbnail = encode(original.thumbnail(400, 400))?;
            let hash = format!("{:x}", Sha256::digest(&image));
            Ok(PreparedUpload {
                image,
                thumbnail,
                hash,
            })
        })
        .await?
    }

    /// Call only inside the shared blocking pool. Check decoder output before allocation.
    pub(crate) fn decode(&self, bytes: &[u8]) -> Result<DynamicImage, ImageError> {
        if bytes.len() > self.limits.max_input_bytes {
            return Err(ImageError::CompressedLimit);
        }
        let mut reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|_| ImageError::Invalid)?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(self.limits.max_dimension);
        limits.max_image_height = Some(self.limits.max_dimension);
        limits.max_alloc = Some(self.limits.max_decoded_bytes);
        reader.limits(limits);
        let decoder = reader.into_decoder().map_err(map_decode_error)?;
        let (width, height) = decoder.dimensions();
        if width == 0
            || height == 0
            || u64::from(width) * u64::from(height) > self.limits.max_pixels
            || decoder.total_bytes() > self.limits.max_decoded_bytes
        {
            return Err(ImageError::DecodedLimit);
        }
        DynamicImage::from_decoder(decoder).map_err(map_decode_error)
    }

    pub(crate) fn decode_for_analysis(&self, bytes: &[u8]) -> Result<DynamicImage, ImageError> {
        let image = self.decode(bytes)?;
        if image.width() > 1200 || image.height() > 1200 {
            Ok(image.resize(1200, 1200, FilterType::Triangle))
        } else {
            Ok(image)
        }
    }
}

fn map_decode_error(error: image::ImageError) -> ImageError {
    match error {
        image::ImageError::Limits(_) => ImageError::DecodedLimit,
        _ => ImageError::Invalid,
    }
}

/// Enforce both declared and actual response bytes, including chunked bodies.
pub async fn read_bounded_response(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> anyhow::Result<Bytes> {
    anyhow::ensure!(
        response
            .content_length()
            .is_none_or(|size| size <= max_bytes as u64),
        "Response exceeds byte limit"
    );
    let mut bytes = BytesMut::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            chunk.len() <= max_bytes.saturating_sub(bytes.len()),
            "Response exceeds byte limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes.freeze())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn supported_formats_still_produce_bounded_webp_images_and_thumbnails() {
        let processor = Arc::new(ImageProcessor::new(
            crate::limits::Limits::bundled().unwrap().images,
        ));
        for format in [
            ImageFormat::Jpeg,
            ImageFormat::Png,
            ImageFormat::WebP,
            ImageFormat::Bmp,
            ImageFormat::Gif,
            ImageFormat::Tiff,
        ] {
            let image = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                16,
                8,
                image::Rgb([32, 64, 96]),
            ));
            let mut bytes = Cursor::new(Vec::new());
            image.write_to(&mut bytes, format).unwrap();
            let prepared = processor
                .prepare_upload(bytes.into_inner().into())
                .await
                .unwrap();
            assert_eq!(
                image::guess_format(&prepared.image).unwrap(),
                ImageFormat::WebP
            );
            assert_eq!(
                image::load_from_memory(&prepared.image).unwrap().width(),
                1200
            );
            assert!(
                image::load_from_memory(&prepared.thumbnail)
                    .unwrap()
                    .width()
                    <= 400
            );
        }
    }

    #[tokio::test]
    async fn large_valid_images_keep_primary_and_thumbnail_dimensions() {
        let processor = Arc::new(ImageProcessor::new(
            crate::limits::Limits::bundled().unwrap().images,
        ));
        let image = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            2400,
            800,
            image::Rgb([32, 64, 96]),
        ));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        let prepared = processor
            .prepare_upload(bytes.into_inner().into())
            .await
            .unwrap();
        let primary = image::load_from_memory(&prepared.image).unwrap();
        let thumbnail = image::load_from_memory(&prepared.thumbnail).unwrap();
        assert_eq!((primary.width(), primary.height()), (1200, 400));
        assert_eq!((thumbnail.width(), thumbnail.height()), (400, 133));
    }

    #[tokio::test]
    async fn http_response_limits_cover_declared_lengths_and_chunked_bodies() {
        use axum::{Router, body::Body, routing::get};
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = server.local_addr().unwrap();
        let app = Router::new()
            .route("/declared", get(|| async { vec![b'x'; 2048] }))
            .route(
                "/chunked",
                get(|| async {
                    Body::from_stream(futures_util::stream::iter(
                        (0..9).map(|_| Ok::<_, std::io::Error>(Bytes::from(vec![b'x'; 128]))),
                    ))
                }),
            )
            .route("/boundary", get(|| async { vec![b'x'; 1024] }));
        let server = tokio::spawn(async move {
            axum::serve(server, app).await.unwrap();
        });
        let client = reqwest::Client::new();
        for path in ["declared", "chunked"] {
            let response = client
                .get(format!("http://{address}/{path}"))
                .send()
                .await
                .unwrap();
            assert!(read_bounded_response(response, 1024).await.is_err());
        }
        let response = client
            .get(format!("http://{address}/boundary"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            read_bounded_response(response, 1024).await.unwrap().len(),
            1024
        );
        server.abort();
    }
}
