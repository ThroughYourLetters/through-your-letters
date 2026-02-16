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
                if let Err(e) = self.process_job(&client, &job).await {
                    tracing::error!(
                        lettering_id = %job.lettering_id,
                        image_url = %job.image_url,
                        "ML processing failed: {}. Job will NOT be retried — lettering remains in current status.",
                        e
                    );
                    // TODO: Consider a dead-letter queue or retry mechanism.
                    // Right now a failed job is lost. The lettering stays in its
                    // current status (likely PENDING) and won't be auto-approved
                    // until the pending_auto_approve worker picks it up.
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    async fn process_job(
        &self,
        client: &reqwest::Client,
        job: &crate::infrastructure::queue::redis_queue::MlJob,
    ) -> anyhow::Result<()> {
        // Fetch image bytes — fail the job if we can't get the image.
        // An empty body is NOT acceptable; it would produce garbage ML results.
        let response =
            client.get(&job.image_url).send().await.map_err(|e| {
                anyhow::anyhow!("Failed to fetch image from {}: {}", job.image_url, e)
            })?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("Image fetch returned HTTP {}: {}", status, job.image_url);
        }

        let bytes = response.bytes().await.map_err(|e| {
            anyhow::anyhow!("Failed to read image body from {}: {}", job.image_url, e)
        })?;

        if bytes.is_empty() {
            anyhow::bail!("Image fetch returned empty body from {}", job.image_url);
        }

        // 1. Text detection: HuggingFace (primary) -> ONNX (fallback) -> default
        let detected_text_str = self.detect_text_with_fallback(client, &bytes).await;

        // 2. Color extraction (local heuristic)
        let colors = self.extract_colors(&bytes);
        let palette = serde_json::to_value(&colors).unwrap_or_default();

        // 3. Style classification (local heuristic, single call)
        let (style, style_confidence) = match self.detector.classify_style(&bytes).await {
            Ok(c) => (c.style, c.confidence),
            Err(e) => {
                tracing::warn!(
                    lettering_id = %job.lettering_id,
                    "Style classification failed: {}. Using 'unknown'.",
                    e
                );
                ("unknown".to_string(), 0.0)
            }
        };

        // 4. Script detection from recognized text
        let script = Self::detect_script(&detected_text_str);

        // 5. Persist results — this is the whole point of the worker.
        //    If this fails, the job has effectively failed.
        sqlx::query!(
            "UPDATE letterings SET detected_text = $1, ml_color_palette = $2, ml_style = $3, ml_script = $4, ml_confidence = $5, status = 'APPROVED', updated_at = NOW() WHERE id = $6",
            &detected_text_str, palette, &style, script, style_confidence, job.lettering_id
        )
        .execute(&self.db)
        .await
        .map_err(|e| anyhow::anyhow!(
            "Failed to persist ML results for lettering {}: {}",
            job.lettering_id, e
        ))?;

        // 6. Broadcast to WebSocket clients.
        //    send() returns Err only when there are zero receivers, which is
        //    normal if no one is connected. That's not an error condition.
        let _ = self
            .broadcaster
            .send(serde_json::json!({"type": "PROCESSED", "id": job.lettering_id}).to_string());

        tracing::info!(
            lettering_id = %job.lettering_id,
            detected_text = %detected_text_str,
            style = %style,
            "ML processing completed successfully"
        );

        Ok(())
    }

    /// Detect text using cascading strategy:
    /// 1. HuggingFace API (primary, if token configured)
    /// 2. ONNX local model (fallback)
    /// 3. Default string (last resort)
    async fn detect_text_with_fallback(
        &self,
        client: &reqwest::Client,
        image_data: &[u8],
    ) -> String {
        // Step 1: Try HuggingFace first
        if self.hf_token.is_some() {
            match self.huggingface_ocr(client, image_data).await {
                Ok(text) if !text.trim().is_empty() => {
                    tracing::info!("HuggingFace OCR succeeded: '{}'", text);
                    return text;
                }
                Ok(text) => {
                    // Model returned successfully but with empty/whitespace text.
                    // This is a valid model response meaning "I see no text."
                    tracing::debug!(
                        "HuggingFace returned empty/whitespace text: '{}'. Trying ONNX.",
                        text
                    );
                }
                Err(e) => {
                    // Infrastructure failure — the model didn't even get a chance.
                    // This is a different situation from "model sees no text."
                    tracing::error!(
                        "HuggingFace OCR infrastructure error: {}. Falling back to ONNX.",
                        e
                    );
                }
            }
        } else {
            tracing::warn!(
                "No HUGGINGFACE_TOKEN configured. HuggingFace is the primary model — \
                 without it, you're running on ONNX fallback only. \
                 Set HUGGINGFACE_TOKEN env var for best results."
            );
        }

        // Step 2: Fall back to ONNX local detection
        match self.detector.detect_text(image_data).await {
            Ok(result)
                if !result.detected_text.is_empty()
                    && result.detected_text != "No text detected"
                    && result.confidence > 0.0 =>
            {
                tracing::info!("ONNX fallback detected text: '{}'", result.detected_text);
                return result.detected_text;
            }
            Ok(result) => {
                tracing::debug!(
                    "ONNX detected no meaningful text (text='{}', confidence={})",
                    result.detected_text,
                    result.confidence
                );
            }
            Err(e) => {
                tracing::warn!("ONNX detection failed: {}", e);
            }
        }

        // Step 3: Last resort fallback
        tracing::info!("All detection methods exhausted, using default text");
        "Handcrafted Lettering".to_string()
    }

    /// Call HuggingFace Inference API for handwritten text OCR.
    ///
    /// Returns `Ok(String)` with the detected text on success (even if empty).
    /// Returns `Err` only for infrastructure failures (network, auth, server errors)
    /// so the caller can distinguish "model found no text" from "couldn't reach model."
    async fn huggingface_ocr(
        &self,
        client: &reqwest::Client,
        data: &[u8],
    ) -> anyhow::Result<String> {
        let token = self
            .hf_token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("HuggingFace token not configured"))?;
        let url = "https://api-inference.huggingface.co/models/microsoft/trocr-base-handwritten";

        let mut last_error = None;

        for attempt in 0..3 {
            let res = client
                .post(url)
                .header("Authorization", format!("Bearer {}", token))
                .body(data.to_vec())
                .send()
                .await
                .map_err(|e| {
                    anyhow::anyhow!("HuggingFace request failed (attempt {}): {}", attempt, e)
                })?;

            let status = res.status();

            if status.is_success() {
                let json: serde_json::Value = res.json().await.map_err(|e| {
                    anyhow::anyhow!("Failed to parse HuggingFace response JSON: {}", e)
                })?;

                let text = json
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|obj| obj.get("generated_text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                return Ok(text);
            }

            if status == StatusCode::SERVICE_UNAVAILABLE {
                // Model is loading (cold start). Wait and retry.
                tracing::info!(
                    "HuggingFace model loading (503), attempt {}/3. Waiting 5s.",
                    attempt + 1
                );
                last_error = Some(anyhow::anyhow!("HuggingFace model loading (503)"));
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }

            if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                // Bad token — this will never work, don't retry.
                anyhow::bail!(
                    "HuggingFace authentication failed (HTTP {}). Check your HUGGINGFACE_TOKEN.",
                    status
                );
            }

            if status == StatusCode::TOO_MANY_REQUESTS {
                // Rate limited. Log prominently — this means we're hitting HF too hard.
                let body = res.text().await.unwrap_or_default();
                anyhow::bail!(
                    "HuggingFace rate limited (HTTP 429): {}. Consider adding request throttling.",
                    body
                );
            }

            // Any other error — read the body for diagnostics and bail.
            let body = res.text().await.unwrap_or_default();
            anyhow::bail!("HuggingFace returned unexpected HTTP {}: {}", status, body);
        }

        // All 3 attempts were 503 (model loading)
        Err(last_error
            .unwrap_or_else(|| anyhow::anyhow!("HuggingFace OCR failed after 3 attempts")))
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
