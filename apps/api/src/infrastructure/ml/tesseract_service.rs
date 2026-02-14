use async_trait::async_trait;
use image::ImageFormat;
use std::io::Cursor;
use super::traits::{MlService, TextDetectionResult, StyleClassification};

pub struct TesseractService {
    enabled: bool,
}

impl TesseractService {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }
    
    fn extract_dominant_colors_from_image(image_data: &[u8]) -> anyhow::Result<Vec<String>> {
        let img = image::load_from_memory(image_data)?;
        let img = img.to_rgb8();
        
        // Simple color extraction - take sample pixels
        let mut colors = Vec::new();
        let (width, height) = img.dimensions();
        
        for y in (0..height).step_by(height as usize / 10) {
            for x in (0..width).step_by(width as usize / 10) {
                let pixel = img.get_pixel(x, y);
                let hex = format!("#{:02X}{:02X}{:02X}", pixel[0], pixel[1], pixel[2]);
                if !colors.contains(&hex) && colors.len() < 5 {
                    colors.push(hex);
                }
            }
        }
        
        Ok(colors)
    }
}

#[async_trait]
impl MlService for TesseractService {
    async fn detect_text(&self, image_data: &[u8]) -> anyhow::Result<TextDetectionResult> {
        if !self.enabled {
            return Ok(TextDetectionResult {
                detected_text: String::new(),
                confidence: 0.0,
                language: None,
            });
        }
        
        // Production OCR using tesseract-rs
        use tesseract::Tesseract;
        
        let tess = Tesseract::new(None, Some("eng+kan+hin+tam+tel"))
            .map_err(|e| anyhow::anyhow!("Tesseract init failed: {}", e))?;
        
        let text = tess
            .set_image_from_mem(image_data)
            .map_err(|e| anyhow::anyhow!("Image load failed: {}", e))?
            .get_text()
            .map_err(|e| anyhow::anyhow!("OCR failed: {}", e))?;
        
        Ok(TextDetectionResult {
            detected_text: text.trim().to_string(),
            confidence: 0.85,
            language: Some("multi".to_string()),
        })
    }
    
    async fn classify_style(&self, image_data: &[u8]) -> anyhow::Result<StyleClassification> {
        if !self.enabled {
            return Ok(StyleClassification {
                style: "unknown".to_string(),
                confidence: 0.0,
            });
        }
        
        // Production style classification using simple heuristics
        let img = image::load_from_memory(image_data)?;
        let gray = img.to_luma8();
        
        // Analyze edge density for style detection
        let edges = imageproc::edges::canny(&gray, 50.0, 100.0);
        let edge_count = edges.pixels().filter(|p| p[0] > 0).count();
        let total_pixels = (edges.width() * edges.height()) as usize;
        let edge_ratio = edge_count as f32 / total_pixels as f32;
        
        let style = if edge_ratio > 0.3 {
            "decorative"
        } else if edge_ratio > 0.15 {
            "handwritten"
        } else {
            "printed"
        };
        
        Ok(StyleClassification {
            style: style.to_string(),
            confidence: 0.75,
        })
    }
    
    async fn extract_colors(&self, image_data: &[u8]) -> anyhow::Result<Vec<String>> {
        if !self.enabled {
            return Ok(vec![]);
        }
        
        Self::extract_dominant_colors_from_image(image_data)
    }
}