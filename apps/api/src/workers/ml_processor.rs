use crate::infrastructure::cache::response_cache::ResponseCache;
use crate::infrastructure::{
    images::read_bounded_response, ml::onnx_text_detector::OnnxTextDetector, ml::traits::MlService,
    queue::postgres_jobs::PostgresJobs,
};
use reqwest::StatusCode;
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tokio::sync::broadcast;

struct AnalysisResult {
    text: String,
    palette: serde_json::Value,
    style: String,
    script: Option<String>,
}

pub struct MlProcessor {
    cache: Arc<ResponseCache>,
    db: PgPool,
    detector: Arc<OnnxTextDetector>,
    queue: Arc<PostgresJobs>,
    hf_token: Option<String>,
    broadcaster: Arc<broadcast::Sender<String>>,
}

impl MlProcessor {
    pub fn new(
        db: PgPool,
        detector: Arc<OnnxTextDetector>,
        queue: Arc<PostgresJobs>,
        hf_token: Option<String>,
        broadcaster: Arc<broadcast::Sender<String>>,
        cache: Arc<ResponseCache>,
    ) -> Self {
        Self {
            cache,
            db,
            detector,
            queue,
            hf_token,
            broadcaster,
        }
    }

    pub async fn start(
        &self,
        shutdown: crate::infrastructure::shutdown::Shutdown,
    ) -> anyhow::Result<()> {
        use crate::infrastructure::queue::postgres_jobs::JobFailure;
        let limits = self.queue.limits();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(limits.http_timeout_seconds))
            .build()?;
        let mut listener = match shutdown.until_requested(self.queue.listener()).await {
            Some(Ok(connection)) => Some(connection),
            Some(Err(_)) => None,
            None => return Ok(()),
        };
        let mut next_recovery = tokio::time::Instant::now();
        let mut queue_failures = 0_u32;
        while !shutdown.requested() {
            if tokio::time::Instant::now() >= next_recovery {
                match shutdown.drain(self.queue.recover_pending()).await {
                    Some(Ok(_)) => {}
                    Some(Err(_)) => tracing::warn!("Pending analysis reconciliation failed"),
                    None => return Ok(()),
                }
                next_recovery =
                    tokio::time::Instant::now() + Duration::from_secs(limits.recovery_seconds);
            }
            if shutdown.requested() {
                return Ok(());
            }
            let Some(claim) = shutdown.drain(self.queue.claim()).await else {
                return Ok(());
            };
            match claim {
                Ok(Some(job)) => {
                    queue_failures = 0;
                    if shutdown.requested() {
                        if !matches!(
                            shutdown.finish(self.queue.release_unstarted(&job)).await,
                            Some(Ok(_))
                        ) {
                            tracing::warn!(lettering_id=%job.lettering_id,"Unstarted claim release unavailable; lease recovery remains active");
                        }
                        return Ok(());
                    }
                    let failure = match shutdown
                        .drain(tokio::time::timeout(
                            Duration::from_secs(limits.job_timeout_seconds),
                            self.process_job(&client, &job),
                        ))
                        .await
                    {
                        Some(Ok(Ok(()))) => None,
                        Some(Ok(Err(_))) => Some(JobFailure::Processing),
                        Some(Err(_)) => Some(JobFailure::Timeout),
                        None => Some(JobFailure::Interrupted),
                    };
                    if let Some(failure) = failure {
                        tracing::warn!(lettering_id=%job.lettering_id,attempt=job.attempt,"Analysis attempt did not complete");
                        if !matches!(
                            shutdown.finish(self.queue.fail(&job, failure)).await,
                            Some(Ok(_))
                        ) {
                            tracing::error!(lettering_id=%job.lettering_id,"Attempt finalization unavailable; lease recovery remains active");
                        }
                    }
                    continue;
                }
                Ok(None) => queue_failures = 0,
                Err(_) => {
                    queue_failures = queue_failures.saturating_add(1);
                    let delay = crate::infrastructure::queue::retry_delay(
                        queue_failures,
                        limits.retry_initial_seconds,
                        limits.retry_max_seconds,
                    );
                    tracing::error!(
                        retry_seconds = delay.as_secs(),
                        "Analysis queue unavailable"
                    );
                    if !shutdown.sleep(delay).await {
                        return Ok(());
                    }
                    continue;
                }
            }
            if listener.is_none() {
                match shutdown.until_requested(self.queue.listener()).await {
                    Some(Ok(connection)) => listener = Some(connection),
                    Some(Err(_)) => {}
                    None => return Ok(()),
                }
            }
            let Some(delay) = shutdown.until_requested(self.queue.next_wake_after()).await else {
                return Ok(());
            };
            let delay = delay.unwrap_or_else(|_| Duration::from_secs(limits.poll_seconds));
            if let Some(connection) = &mut listener {
                match shutdown
                    .until_requested(tokio::time::timeout(delay, connection.recv()))
                    .await
                {
                    None => return Ok(()),
                    Some(Ok(Err(_))) => listener = None,
                    _ => {}
                }
            } else if !shutdown.sleep(delay).await {
                return Ok(());
            }
        }
        Ok(())
    }

    /// Process one upload; reviewed or non-pending records stay unchanged.
    pub async fn process_job(
        &self,
        client: &reqwest::Client,
        job: &crate::infrastructure::queue::postgres_jobs::ClaimedJob,
    ) -> anyhow::Result<()> {
        let pending: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM letterings l JOIN analysis_jobs j ON j.lettering_id=l.id
             WHERE l.id=$1 AND l.status='PENDING' AND l.moderated_at IS NULL
             AND j.state='running' AND j.lease_token=$2 AND j.lease_expires_at>clock_timestamp())",
        )
        .bind(job.lettering_id)
        .bind(job.lease_token)
        .fetch_one(&self.db)
        .await?;
        if !pending {
            self.finish(job, None).await?;
            return Ok(());
        }

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

        let bytes =
            read_bounded_response(response, self.detector.image_limits().max_input_bytes).await?;

        if bytes.is_empty() {
            anyhow::bail!("Image fetch returned empty body from {}", job.image_url);
        }

        // Decode limits apply before any external OCR request.
        let colors = self.detector.extract_colors(bytes.clone()).await?;
        let palette = serde_json::to_value(&colors)?;
        let detected_text_str = self.detect_text_with_fallback(client, &bytes).await?;
        let style = self.detector.classify_style(bytes).await?.style;

        let script = Self::detect_script(&detected_text_str);

        self.finish(
            job,
            Some(&AnalysisResult {
                text: detected_text_str,
                palette,
                style,
                script,
            }),
        )
        .await?;
        Ok(())
    }

    async fn finish(
        &self,
        job: &crate::infrastructure::queue::postgres_jobs::ClaimedJob,
        analysis: Option<&AnalysisResult>,
    ) -> anyhow::Result<()> {
        let mut tx = self.db.begin().await?;
        // Parent before job matches cascading deletion and avoids holding locks during inference.
        let parent =
            sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM letterings WHERE id=$1 FOR UPDATE")
                .bind(job.lettering_id)
                .fetch_optional(&mut *tx)
                .await?;
        if parent.is_none() || !PostgresJobs::acknowledge_in(&mut tx, job).await? {
            return Ok(());
        }
        let Some(analysis) = analysis else {
            tx.commit().await?;
            return Ok(());
        };
        let style_confidence: Option<f32> = None;
        let result = sqlx::query!(
            "UPDATE letterings SET detected_text = $1, ml_color_palette = $2, ml_style = $3, ml_script = $4, ml_confidence = $5, status = 'APPROVED', updated_at = NOW() WHERE id = $6 AND status = 'PENDING' AND moderated_at IS NULL",
            &analysis.text, &analysis.palette, &analysis.style, analysis.script.as_deref(), style_confidence, job.lettering_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow::anyhow!(
            "Failed to persist ML results for lettering {}: {}",
            job.lettering_id, e
        ))?;

        tx.commit().await?;
        if result.rows_affected() == 0 {
            return Ok(());
        }
        self.cache.invalidate().await;

        // No receivers is normal when no client is connected.
        let _ = self
            .broadcaster
            .send(serde_json::json!({"type": "PROCESSED", "id": job.lettering_id}).to_string());

        tracing::info!(lettering_id = %job.lettering_id, "ML processing completed successfully");

        Ok(())
    }

    /// Detect text using cascading strategy:
    /// 1. HuggingFace API (primary, if token configured)
    /// 2. ONNX local model (fallback)
    /// 3. Default string (last resort)
    async fn detect_text_with_fallback(
        &self,
        client: &reqwest::Client,
        image_data: &bytes::Bytes,
    ) -> anyhow::Result<String> {
        // Step 1: Try HuggingFace first
        if self.hf_token.is_some() {
            match self.huggingface_ocr(client, image_data).await {
                Ok(text) if !text.trim().is_empty() => {
                    tracing::info!("HuggingFace OCR succeeded");
                    return Ok(text);
                }
                Ok(_) => {
                    // Model returned successfully but with empty/whitespace text.
                    // This is a valid model response meaning "I see no text."
                    tracing::debug!("HuggingFace returned no text; trying ONNX");
                }
                Err(_) => {
                    // Infrastructure failure — the model didn't even get a chance.
                    // This is a different situation from "model sees no text."
                    tracing::error!("HuggingFace OCR failed; falling back to ONNX");
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
        match self.detector.detect_text(image_data.clone()).await {
            Ok(result)
                if !result.detected_text.is_empty()
                    && result.detected_text != "No text detected"
                    && result.confidence > 0.0 =>
            {
                tracing::info!("ONNX fallback detected text");
                return Ok(result.detected_text);
            }
            Ok(_) => {
                tracing::debug!("ONNX detected no meaningful text");
            }
            Err(error) => {
                if error
                    .downcast_ref::<crate::infrastructure::blocking::BlockingError>()
                    .is_some()
                {
                    return Err(error);
                }
                tracing::warn!("ONNX detection failed");
            }
        }

        // Step 3: Last resort fallback
        tracing::info!("All detection methods exhausted, using default text");
        Ok("Handcrafted Lettering".to_string())
    }

    /// Call HuggingFace Inference API for handwritten text OCR.
    ///
    /// Returns `Ok(String)` with the detected text on success (even if empty).
    /// Returns `Err` only for infrastructure failures (network, auth, server errors)
    /// so the caller can distinguish "model found no text" from "couldn't reach model."
    async fn huggingface_ocr(
        &self,
        client: &reqwest::Client,
        data: &bytes::Bytes,
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
                .body(data.clone())
                .send()
                .await
                .map_err(|e| {
                    anyhow::anyhow!("HuggingFace request failed (attempt {}): {}", attempt, e)
                })?;

            let status = res.status();

            if status.is_success() {
                let bytes = read_bounded_response(
                    res,
                    self.detector.image_limits().max_provider_response_bytes,
                )
                .await?;
                let json: serde_json::Value = serde_json::from_slice(&bytes)?;

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
                anyhow::bail!("HuggingFace rate limited (HTTP 429)");
            }

            anyhow::bail!("HuggingFace returned unexpected HTTP {}", status);
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
}
