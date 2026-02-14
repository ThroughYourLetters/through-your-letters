use super::traits::{MlService, StyleClassification, TextDetectionResult};
use async_trait::async_trait;
use image::imageops::FilterType;
use ndarray::{Array, IxDyn};
use ort::{session::Session, value::Value};
use std::path::Path;
use std::sync::Mutex;

pub struct OnnxTextDetector {
    // Wrap Session in Mutex to allow mutable access (run) from immutable &self
    session: Option<Mutex<Session>>,
    enabled: bool,
}

impl OnnxTextDetector {
    pub fn new(model_path: &str, enabled: bool) -> anyhow::Result<Self> {
        if !enabled || !Path::new(model_path).exists() {
            if enabled {
                tracing::warn!(
                    "ML model file not found at {}. Local pre-checks will be skipped, falling back to primary OCR.",
                    model_path
                );
            }
            return Ok(Self {
                session: None,
                enabled: false,
            });
        }

        let session = Session::builder()?.commit_from_file(model_path)?;

        Ok(Self {
            session: Some(Mutex::new(session)),
            enabled: true,
        })
    }

    fn preprocess_image(&self, image_data: &[u8]) -> anyhow::Result<Array<f32, IxDyn>> {
        let img = image::load_from_memory(image_data)?;
        let img_resized = img.resize_exact(640, 640, FilterType::Triangle);
        let img_rgb = img_resized.to_rgb8();

        let (width, height) = img_rgb.dimensions();
        let mut array = Array::zeros(IxDyn(&[1, 3, height as usize, width as usize]));

        for y in 0..height {
            for x in 0..width {
                let pixel = img_rgb.get_pixel(x, y);
                array[[0, 0, y as usize, x as usize]] = pixel[0] as f32 / 255.0;
                array[[0, 1, y as usize, x as usize]] = pixel[1] as f32 / 255.0;
                array[[0, 2, y as usize, x as usize]] = pixel[2] as f32 / 255.0;
            }
        }

        Ok(array)
    }

    fn extract_text_from_detections(&self, output: &Array<f32, IxDyn>) -> String {
        let shape = output.shape();
        if shape.len() < 2 {
            return String::new();
        }

        let threshold = 0.5;
        let mut detected_regions = Vec::new();

        if shape.len() == 3 && shape[2] >= 5 {
            for detection_idx in 0..shape[1] {
                let confidence = output[[0, detection_idx, 4]];
                if confidence > threshold {
                    detected_regions.push(format!("text_region_{}", detected_regions.len()));
                }
            }
        }

        if detected_regions.is_empty() {
            "No text detected".to_string()
        } else {
            format!("Detected {} text regions", detected_regions.len())
        }
    }
}

#[async_trait]
impl MlService for OnnxTextDetector {
    async fn detect_text(&self, image_data: &[u8]) -> anyhow::Result<TextDetectionResult> {
        if !self.enabled || self.session.is_none() {
            return Ok(TextDetectionResult {
                detected_text: String::new(),
                confidence: 0.0,
                language: None,
            });
        }

        // Prepare input
        let input_tensor = self.preprocess_image(image_data)?;

        // Manual conversion (Shape + Data) to avoid version mismatch errors
        let input_shape: Vec<i64> = input_tensor.shape().iter().map(|&d| d as i64).collect();
        let input_data = input_tensor.into_raw_vec();
        let input_value = Value::from_array((input_shape, input_data))?;

        // LOCK THE SESSION
        // We need a mutable reference to run the session, so we lock the Mutex.
        let session_mutex = self.session.as_ref().unwrap();
        let mut session = session_mutex
            .lock()
            .map_err(|_| anyhow::anyhow!("Failed to acquire session lock"))?;

        // Run inference
        let outputs = session.run(ort::inputs![input_value])?;

        // Manual output conversion
        let (extract_shape, extract_data) = outputs[0].try_extract_tensor::<f32>()?;

        // Reconstruct ndarray
        let shape_vec: Vec<usize> = extract_shape.iter().map(|&d| d as usize).collect();
        let output_array = Array::from_shape_vec(IxDyn(&shape_vec), extract_data.to_vec())?;

        let detected_text = self.extract_text_from_detections(&output_array);
        let confidence = if detected_text.is_empty() || detected_text == "No text detected" {
            0.0
        } else {
            0.85
        };

        Ok(TextDetectionResult {
            detected_text,
            confidence,
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

        let img = image::load_from_memory(image_data)?;
        let gray = img.to_luma8();

        let mut edge_count = 0;
        let (width, height) = gray.dimensions();

        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let center = gray.get_pixel(x, y)[0] as i32;
                let left = gray.get_pixel(x - 1, y)[0] as i32;
                let right = gray.get_pixel(x + 1, y)[0] as i32;
                let top = gray.get_pixel(x, y - 1)[0] as i32;
                let bottom = gray.get_pixel(x, y + 1)[0] as i32;

                let gradient = ((center - left).abs()
                    + (center - right).abs()
                    + (center - top).abs()
                    + (center - bottom).abs())
                    / 4;

                if gradient > 30 {
                    edge_count += 1;
                }
            }
        }

        let total_pixels = (width * height) as f32;
        let edge_ratio = edge_count as f32 / total_pixels;

        // Compute color variance for additional classification
        let rgb = img.to_rgb8();
        let mut r_sum: f64 = 0.0;
        let mut g_sum: f64 = 0.0;
        let mut b_sum: f64 = 0.0;
        let mut count: f64 = 0.0;
        for y in (0..height).step_by(5) {
            for x in (0..width).step_by(5) {
                let p = rgb.get_pixel(x, y);
                r_sum += p[0] as f64;
                g_sum += p[1] as f64;
                b_sum += p[2] as f64;
                count += 1.0;
            }
        }
        let avg_brightness = (r_sum + g_sum + b_sum) / (3.0 * count);
        let color_variance = ((r_sum / count - g_sum / count).powi(2)
            + (g_sum / count - b_sum / count).powi(2)
            + (r_sum / count - b_sum / count).powi(2))
        .sqrt();

        let style = if edge_ratio > 0.35 && color_variance > 40.0 {
            "graffiti"
        } else if edge_ratio > 0.3 && color_variance > 30.0 {
            "neon"
        } else if edge_ratio > 0.3 {
            "decorative"
        } else if edge_ratio > 0.2 && avg_brightness < 80.0 {
            "carved"
        } else if edge_ratio > 0.2 && color_variance < 15.0 {
            "stenciled"
        } else if edge_ratio > 0.15 && color_variance > 20.0 {
            "hand-painted"
        } else if edge_ratio > 0.15 {
            "calligraphic"
        } else if edge_ratio > 0.08 {
            "typeface-printed"
        } else if avg_brightness > 200.0 {
            "chalk"
        } else {
            "digital-print"
        };

        Ok(StyleClassification {
            style: style.to_string(),
            confidence: 0.75,
        })
    }

    async fn extract_colors(&self, image_data: &[u8]) -> anyhow::Result<Vec<String>> {
        let img = image::load_from_memory(image_data)?;
        let img = img.to_rgb8();

        let (width, height) = img.dimensions();
        let mut color_counts = std::collections::HashMap::new();

        for y in (0..height).step_by(10) {
            for x in (0..width).step_by(10) {
                let pixel = img.get_pixel(x, y);
                let r = (pixel[0] / 32) * 32;
                let g = (pixel[1] / 32) * 32;
                let b = (pixel[2] / 32) * 32;
                let hex = format!("#{:02X}{:02X}{:02X}", r, g, b);
                *color_counts.entry(hex).or_insert(0) += 1;
            }
        }

        let mut colors: Vec<_> = color_counts.into_iter().collect();
        colors.sort_by(|a, b| b.1.cmp(&a.1));

        Ok(colors.into_iter().take(5).map(|(color, _)| color).collect())
    }
}
