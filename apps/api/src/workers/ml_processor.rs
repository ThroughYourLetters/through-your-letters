use crate::infrastructure::{
    ml::onnx_text_detector::OnnxTextDetector, ml::traits::MlService, queue::redis_queue::RedisQueue,
};
use reqwest::StatusCode;
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tokio::sync::broadcast;

pub struct MlProcessor {
    db: PgPool,
    detector: Arc<OnnxTextDetector>,
    queue: Arc<RedisQueue>,
    hf_token: Option<String>,
    broadcaster: Arc<broadcast::Sender<String>>,
}

impl MlProcessor {
    pub fn new(
        db: PgPool,
        detector: Arc<OnnxTextDetector>,
        queue: Arc<RedisQueue>,
        hf_token: Option<String>,
        broadcaster: Arc<broadcast::Sender<String>>,
    ) -> Self {
        Self {
            db,
            detector,
            queue,
            hf_token,
            broadcaster,
        }
    }

    pub async fn start(&self) {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap();
        loop {
            if let Ok(Some(job)) = self.queue.dequeue_ml_job().await {
                let bytes = match client.get(&job.image_url).send().await {
                    Ok(res) => res.bytes().await.unwrap_or_default(),
                    _ => continue,
                };

                // Local Presence Check (Secondary)
                let onnx_says_text = self.detector.detect_text(&bytes).await.is_ok();

                // Primary: HuggingFace Handwriting OCR
                let mut text = if onnx_says_text {
                    self.huggingface_ocr(&client, &bytes).await
                } else {
                    None
                };

                if text.is_none() && onnx_says_text {
                    text = Some("Handcrafted Lettering".into());
                }

                let colors = self.extract_colors(&bytes);
                let palette = serde_json::to_value(&colors).unwrap_or_default();

                // Classify style
                let style = self
                    .detector
                    .classify_style(&bytes)
                    .await
                    .map(|s| s.style)
                    .unwrap_or_else(|_| "unknown".into());
                let style_confidence = self
                    .detector
                    .classify_style(&bytes)
                    .await
                    .map(|s| s.confidence)
                    .unwrap_or(0.0);

                // Detect script from recognized text
                let detected_text_str = text.as_deref().unwrap_or("Street Discovery");
                let script = Self::detect_script(detected_text_str);

                let _ = sqlx::query!(
                    "UPDATE letterings SET detected_text = $1, ml_color_palette = $2, ml_style = $3, ml_script = $4, ml_confidence = $5, status = 'APPROVED', updated_at = NOW() WHERE id = $6",
                    detected_text_str, palette, style, script, style_confidence, job.lettering_id
                ).execute(&self.db).await;

                let _ = self.broadcaster.send(
                    serde_json::json!({"type": "PROCESSED", "id": job.lettering_id}).to_string(),
                );
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    async fn huggingface_ocr(&self, client: &reqwest::Client, data: &[u8]) -> Option<String> {
        let token = self.hf_token.as_ref()?;
        let url = "https://api-inference.huggingface.co/models/microsoft/trocr-base-handwritten";

        for _ in 0..3 {
            let res = client
                .post(url)
                .header("Authorization", format!("Bearer {}", token))
                .body(data.to_vec())
                .send()
                .await
                .ok()?;

            if res.status().is_success() {
                let json: serde_json::Value = res.json().await.ok()?;
                return json
                    .as_array()?
                    .first()?
                    .get("generated_text")?
                    .as_str()
                    .map(|s| s.to_string());
            } else if res.status() == StatusCode::SERVICE_UNAVAILABLE {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            break;
        }
        None
    }

    fn detect_script(text: &str) -> Option<String> {
        let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for ch in text.chars() {
            let script = match ch as u32 {
                0x0900..=0x097F => Some("Devanagari"),
                0x0980..=0x09FF => Some("Bengali"),
                0x0A00..=0x0A7F => Some("Gurmukhi"),
                0x0A80..=0x0AFF => Some("Gujarati"),
                0x0B00..=0x0B7F => Some("Odia"),
                0x0B80..=0x0BFF => Some("Tamil"),
                0x0C00..=0x0C7F => Some("Telugu"),
                0x0C80..=0x0CFF => Some("Kannada"),
                0x0D00..=0x0D7F => Some("Malayalam"),
                0x0600..=0x06FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => Some("Arabic/Urdu"),
                0x0041..=0x007A => Some("Latin"),
                _ => None,
            };
            if let Some(s) = script {
                *counts.entry(s).or_insert(0) += 1;
            }
        }
        counts
            .into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(s, _)| s.to_string())
    }

    fn extract_colors(&self, data: &[u8]) -> Vec<String> {
        if let Ok(img) = image::load_from_memory(data).map(|i| i.to_rgb8()) {
            let mut counts = std::collections::HashMap::new();
            for y in (0..img.height()).step_by(25) {
                for x in (0..img.width()).step_by(25) {
                    let p = img.get_pixel(x, y);
                    let hex = format!(
                        "#{:02X}{:02X}{:02X}",
                        (p[0] / 32) * 32,
                        (p[1] / 32) * 32,
                        (p[2] / 32) * 32
                    );
                    *counts.entry(hex).or_insert(0) += 1;
                }
            }
            let mut v: Vec<_> = counts.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            v.into_iter().take(3).map(|(k, _)| k).collect()
        } else {
            vec![]
        }
    }
}
