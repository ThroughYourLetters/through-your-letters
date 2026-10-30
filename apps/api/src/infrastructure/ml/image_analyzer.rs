use crate::infrastructure::images::ImageProcessor;
use bytes::Bytes;
use image::imageops::FilterType;
use ndarray::{Array, IxDyn};
use ort::{session::Session, value::Value};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ImageAnalyzer {
    model: Option<Arc<LoadedModel>>,
    images: Arc<ImageProcessor>,
}

struct LoadedModel {
    session: Mutex<Session>,
    input_width: u32,
    input_height: u32,
}

pub struct StyleClassification {
    pub style: String,
}

impl ImageAnalyzer {
    pub fn image_limits(&self) -> &crate::limits::ImageLimits {
        self.images.limits()
    }

    /// Load an optional local model with one RGB NCHW input. Missing or disabled
    /// models leave local style hints unavailable; OCR is configured separately.
    pub fn new(
        model_path: &str,
        enabled: bool,
        images: Arc<ImageProcessor>,
    ) -> anyhow::Result<Self> {
        if !enabled || !Path::new(model_path).exists() {
            if enabled {
                tracing::warn!("Local model is unavailable; local style hints are disabled");
            }
            return Ok(Self {
                model: None,
                images,
            });
        }
        let session = Session::builder()?
            .with_intra_threads(1)?
            .with_inter_threads(1)?
            .commit_from_file(model_path)?;
        anyhow::ensure!(session.inputs().len() == 1, "Expected one model input");
        let shape = session.inputs()[0]
            .dtype()
            .tensor_shape()
            .ok_or_else(|| anyhow::anyhow!("Expected a tensor model input"))?;
        anyhow::ensure!(
            shape.len() == 4 && matches!(shape[0], 1 | -1) && matches!(shape[1], 3 | -1),
            "Expected model input [1,3,height,width]"
        );
        let dimension = |value| -> anyhow::Result<u32> {
            if value == -1 {
                return Ok(640);
            }
            let value = u32::try_from(value)?;
            anyhow::ensure!(
                (1..=1024).contains(&value),
                "Model image dimensions must be 1..1024"
            );
            Ok(value)
        };
        let input_height = dimension(shape[2])?;
        let input_width = dimension(shape[3])?;
        Ok(Self {
            model: Some(Arc::new(LoadedModel {
                session: Mutex::new(session),
                input_width,
                input_height,
            })),
            images,
        })
    }

    /// Decode and resize an image to this model's input dimensions, returning
    /// RGB floats from 0 to 1 with shape `[1, 3, height, width]`. Decode failures propagate.
    fn preprocess_image(
        &self,
        image_data: &[u8],
        model: &LoadedModel,
    ) -> anyhow::Result<Array<f32, IxDyn>> {
        let img = self.images.decode(image_data)?;
        let img_resized =
            img.resize_exact(model.input_width, model.input_height, FilterType::Triangle);
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
}

impl ImageAnalyzer {
    fn check_model_sync(&self, image_data: &[u8]) -> anyhow::Result<()> {
        let model = self
            .model
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Local model is unavailable"))?;
        let input = self.preprocess_image(image_data, model)?;
        let shape: Vec<i64> = input.shape().iter().map(|&value| value as i64).collect();
        let (data, _) = input.into_raw_vec_and_offset();
        let value = Value::from_array((shape, data))?;
        let mut session = model
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("Local model session is unavailable"))?;
        let outputs = session.run(ort::inputs![value])?;
        anyhow::ensure!(
            outputs.len() == 3,
            "Expected boxes, class IDs, and scores from the local model"
        );
        let (box_shape, boxes) = outputs[0].try_extract_tensor::<f32>()?;
        let (class_shape, classes) = outputs[1].try_extract_tensor::<i64>()?;
        let (score_shape, scores) = outputs[2].try_extract_tensor::<f32>()?;
        let count = i64::try_from(classes.len())?;
        anyhow::ensure!(
            &box_shape[..] == [count, 4].as_slice()
                && &class_shape[..] == [count].as_slice()
                && &score_shape[..] == [count].as_slice(),
            "Inconsistent local model detection shapes"
        );
        anyhow::ensure!(
            boxes.iter().all(|value| value.is_finite())
                && scores
                    .iter()
                    .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
                && classes.iter().all(|&value| value >= 0),
            "Invalid local model detections"
        );
        Ok(())
    }

    fn classify_style_sync(&self, image_data: &[u8]) -> anyhow::Result<StyleClassification> {
        if self.model.is_none() {
            return Ok(StyleClassification {
                style: "unknown".to_string(),
            });
        }

        let img = self.images.decode_for_analysis(image_data)?;
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
        })
    }

    /// Sample every twenty-fifth pixel in each direction and return up to three frequent
    /// colors, with RGB channels rounded down to multiples of 32. Invalid images
    /// return an error.
    fn extract_colors_sync(&self, image_data: &[u8]) -> anyhow::Result<Vec<String>> {
        let img = self.images.decode_for_analysis(image_data)?;
        let img = img.to_rgb8();

        let (width, height) = img.dimensions();
        let mut color_counts = std::collections::HashMap::new();

        for y in (0..height).step_by(25) {
            for x in (0..width).step_by(25) {
                let pixel = img.get_pixel(x, y);
                let r = (pixel[0] / 32) * 32;
                let g = (pixel[1] / 32) * 32;
                let b = (pixel[2] / 32) * 32;
                let hex = format!("#{:02X}{:02X}{:02X}", r, g, b);
                *color_counts.entry(hex).or_insert(0) += 1;
            }
        }

        let mut colors: Vec<_> = color_counts.into_iter().collect();
        colors.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        Ok(colors.into_iter().take(3).map(|(color, _)| color).collect())
    }
}

impl ImageAnalyzer {
    /// Run the native model and validate its detection tensors. Class IDs and
    /// model scores have no verified mapping to recognized words.
    pub async fn check_model(&self, bytes: Bytes) -> anyhow::Result<()> {
        let analyzer = self.clone();
        self.images
            .run(move || analyzer.check_model_sync(&bytes))
            .await?
    }

    pub async fn classify_style(&self, bytes: Bytes) -> anyhow::Result<StyleClassification> {
        let analyzer = self.clone();
        self.images
            .run(move || analyzer.classify_style_sync(&bytes))
            .await?
    }

    pub async fn extract_colors(&self, bytes: Bytes) -> anyhow::Result<Vec<String>> {
        let analyzer = self.clone();
        self.images
            .run(move || analyzer.extract_colors_sync(&bytes))
            .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use std::io::Cursor;

    #[tokio::test]
    async fn absent_models_keep_style_unknown_and_palette_ties_are_deterministic() {
        let image = RgbImage::from_fn(75, 1, |x, _| match x / 25 {
            0 => Rgb([255, 0, 0]),
            1 => Rgb([0, 255, 0]),
            _ => Rgb([0, 0, 255]),
        });
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(image)
            .write_to(&mut encoded, ImageFormat::Png)
            .unwrap();
        let bytes = Bytes::from(encoded.into_inner());
        for enabled in [false, true] {
            let images = Arc::new(ImageProcessor::new(
                crate::limits::Limits::bundled().unwrap().images,
            ));
            let analyzer = ImageAnalyzer::new("missing-fixture-model", enabled, images).unwrap();
            assert!(analyzer.check_model(bytes.clone()).await.is_err());
            assert_eq!(
                analyzer.classify_style(bytes.clone()).await.unwrap().style,
                "unknown"
            );
            assert_eq!(
                analyzer.extract_colors(bytes.clone()).await.unwrap(),
                ["#0000E0", "#00E000", "#E00000"]
            );
        }
    }
}
