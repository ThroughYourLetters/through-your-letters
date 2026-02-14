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
