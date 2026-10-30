use async_trait::async_trait;
use bytes::Bytes;

#[derive(Debug, Clone)]
pub struct TextDetectionResult {
    pub detected_text: String,
    pub confidence: f32,
    pub language: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StyleClassification {
    pub style: String,
}

#[async_trait]
pub trait MlService: Send + Sync {
    /// Detect text in image using OCR
    async fn detect_text(&self, image_data: Bytes) -> anyhow::Result<TextDetectionResult>;

    /// Classify lettering style
    async fn classify_style(&self, image_data: Bytes) -> anyhow::Result<StyleClassification>;

    /// Extract dominant colors
    async fn extract_colors(&self, image_data: Bytes) -> anyhow::Result<Vec<String>>;
}
