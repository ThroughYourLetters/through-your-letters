use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlJob {
    pub lettering_id: Uuid,
    pub image_url: String,
}

pub struct RedisQueue {
    client: Client,
}
impl RedisQueue {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
    pub async fn enqueue_ml_job(&self, job: MlJob) -> anyhow::Result<()> {
        let mut conn = tokio::time::timeout(
            Duration::from_secs(5),
            self.client.get_multiplexed_async_connection(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Redis connection timed out"))??;
        let _: usize = conn.lpush("ml_jobs", serde_json::to_string(&job)?).await?;
        Ok(())
    }
    pub async fn dequeue_ml_job(&self) -> anyhow::Result<Option<MlJob>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let res: Option<(String, String)> = conn.brpop("ml_jobs", 5.0).await?;
        match res {
            Some((_, json)) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }
}
