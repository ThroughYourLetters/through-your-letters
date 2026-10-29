use super::traits::StorageService;
use async_trait::async_trait;
use aws_sdk_s3::{
    Client, config::BehaviorVersion, config::Credentials, config::Region, primitives::ByteStream,
};

pub struct R2StorageService {
    client: Client,
    bucket: String,
    public_url: String,
}

impl R2StorageService {
    pub async fn new(
        key: String,
        secret: String,
        endpoint: String,
        region: String,
        force_path_style: bool,
        bucket: String,
        public_url: String,
    ) -> anyhow::Result<Self> {
        let creds = Credentials::new(key, secret, None, None, "r2");
        let config = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .credentials_provider(creds)
            .endpoint_url(endpoint)
            .region(Region::new(region))
            .force_path_style(force_path_style)
            .build();
        Ok(Self {
            client: Client::from_conf(config),
            bucket,
            public_url,
        })
    }
}

#[async_trait]
impl StorageService for R2StorageService {
    async fn upload(&self, key: &str, data: Vec<u8>, content_type: &str) -> anyhow::Result<String> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(data))
            .content_type(content_type)
            .cache_control("public, max-age=31536000, immutable")
            .send()
            .await?;
        Ok(format!("{}/{}", self.public_url, key))
    }
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;
        Ok(())
    }
    fn get_url(&self, key: &str) -> String {
        format!("{}/{}", self.public_url, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Bytes,
        http::{HeaderMap, Method, StatusCode, Uri},
        routing::any,
    };

    /// Exercise the actual SDK connector and SigV4 signer without a cloud account.
    #[tokio::test]
    async fn updated_sdk_preserves_r2_upload_and_delete_requests() {
        let (sender, mut received) = tokio::sync::mpsc::channel(2);
        let fixture = Router::new().route(
            "/{*key}",
            any(
                move |method: Method, uri: Uri, headers: HeaderMap, body: Bytes| {
                    let sender = sender.clone();
                    async move {
                        sender.send((method, uri, headers, body)).await.unwrap();
                        StatusCode::OK
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, fixture).await.unwrap() });
        let storage = R2StorageService::new(
            "fixture-key".into(),
            "fixture-secret".into(),
            endpoint,
            "auto".into(),
            true,
            "archive".into(),
            "https://images.example.test".into(),
        )
        .await
        .unwrap();
        let url = storage
            .upload(
                "letterings/test.webp",
                b"fixture-image".to_vec(),
                "image/webp",
            )
            .await
            .unwrap();
        assert_eq!(url, "https://images.example.test/letterings/test.webp");
        storage.delete("letterings/test.webp").await.unwrap();
        for expected in [Method::PUT, Method::DELETE] {
            let (method, uri, headers, _body) = received.recv().await.unwrap();
            assert_eq!(method, expected);
            assert_eq!(uri.path(), "/archive/letterings/test.webp");
            let authorization = headers["authorization"].to_str().unwrap();
            assert!(authorization.starts_with("AWS4-HMAC-SHA256 "));
            assert!(authorization.contains("/auto/s3/aws4_request"));
            if method == Method::PUT {
                assert_eq!(headers["content-type"], "image/webp");
                assert_eq!(
                    headers["cache-control"],
                    "public, max-age=31536000, immutable"
                );
            }
        }
        server.abort();
        let _ = server.await;
    }
}
