use crate::infrastructure::images::read_bounded_response;
use bytes::Bytes;
use serde::Deserialize;

/// Recognized words from a configured image-to-text provider. Local object
/// detections are not a substitute for recognition.
pub struct OcrService {
    endpoint: reqwest::Url,
    token: String,
    max_response_bytes: usize,
}

#[derive(Deserialize)]
struct Recognition {
    generated_text: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RecognitionResponse {
    Single(Recognition),
    Batch(Vec<Recognition>),
}

impl OcrService {
    pub fn new(endpoint: reqwest::Url, token: String, max_response_bytes: usize) -> Self {
        Self {
            endpoint,
            token,
            max_response_bytes,
        }
    }

    pub async fn recognize(
        &self,
        client: &reqwest::Client,
        image: &Bytes,
    ) -> anyhow::Result<Option<String>> {
        // The durable job owns retry policy; one attempt makes one provider call.
        let response = client
            .post(self.endpoint.clone())
            .bearer_auth(&self.token)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(image.clone())
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "OCR provider rejected the request"
        );
        let body = read_bounded_response(response, self.max_response_bytes).await?;
        let result: RecognitionResponse = serde_json::from_slice(&body)?;
        let recognition = match result {
            RecognitionResponse::Single(result) => result,
            RecognitionResponse::Batch(mut results) => {
                anyhow::ensure!(
                    results.len() == 1,
                    "OCR provider returned an unexpected batch size"
                );
                results.remove(0)
            }
        };
        let text = recognition.generated_text.trim();
        Ok((!text.is_empty()).then(|| text.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        extract::{Request, State},
        http::StatusCode,
        response::Response,
        routing::post,
    };
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    struct Reply {
        calls: AtomicUsize,
        status: StatusCode,
        body: &'static str,
    }

    async fn provider(State(reply): State<Arc<Reply>>, request: Request) -> Response {
        reply.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request.headers()["authorization"], "Bearer fixture-token");
        assert_eq!(
            &axum::body::to_bytes(request.into_body(), 100)
                .await
                .unwrap()[..],
            b"fixture-image"
        );
        Response::builder()
            .status(reply.status)
            .body(Body::from(reply.body))
            .unwrap()
    }

    async fn check(
        status: StatusCode,
        body: &'static str,
        limit: usize,
    ) -> anyhow::Result<Option<String>> {
        let reply = Arc::new(Reply {
            calls: AtomicUsize::new(0),
            status,
            body,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/ocr", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let router = Router::new()
            .route("/ocr", post(provider))
            .with_state(reply.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let service = OcrService::new(endpoint, "fixture-token".into(), limit);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let result = service
            .recognize(&client, &Bytes::from_static(b"fixture-image"))
            .await;
        assert_eq!(reply.calls.load(Ordering::SeqCst), 1);
        server.abort();
        result
    }

    #[tokio::test]
    async fn recognition_preserves_actual_words_and_distinguishes_empty_text() {
        for body in [
            r#"[{"generated_text":"  नमस्ते  "}]"#,
            r#"{"generated_text":"  नमस्ते  "}"#,
        ] {
            assert_eq!(
                check(StatusCode::OK, body, 1024).await.unwrap(),
                Some("नमस्ते".into())
            );
        }
        assert_eq!(
            check(StatusCode::OK, r#"[{"generated_text":"  "}]"#, 1024)
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn provider_failures_and_malformed_results_never_become_recognized_text() {
        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::SERVICE_UNAVAILABLE,
        ] {
            assert!(
                check(status, r#"{"error":"private provider details"}"#, 1024)
                    .await
                    .is_err()
            );
        }
        for body in [
            "not JSON",
            r#"{"error":"private provider details"}"#,
            "[]",
            r#"[{"generated_text":"a"},{"generated_text":"b"}]"#,
        ] {
            assert!(check(StatusCode::OK, body, 1024).await.is_err());
        }
        assert!(
            check(StatusCode::OK, r#"{"generated_text":"too long"}"#, 8)
                .await
                .is_err()
        );
    }
}
