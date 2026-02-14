use async_trait::async_trait;

#[async_trait]
pub trait StorageService: Send + Sync {
    async fn upload(&self, key: &str, data: Vec<u8>, content_type: &str) -> anyhow::Result<String>;
    async fn delete(&self, key: &str) -> anyhow::Result<()>;
    fn get_url(&self, key: &str) -> String;
}
