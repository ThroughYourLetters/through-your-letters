use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

const ML_JOBS_KEY: &str = "ml_jobs";
const ML_JOBS_DLQ_KEY: &str = "ml_jobs_dlq";
const REDIS_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlJob {
    pub lettering_id: Uuid,
    pub image_url: String,
    /// Number of times this job has already been attempted. Zero on first enqueue.
    #[serde(default)]
    pub retry_count: u32,
}

pub struct RedisQueue {
    client: Client,
}

impl RedisQueue {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    async fn connection(&self) -> anyhow::Result<redis::aio::MultiplexedConnection> {
        tokio::time::timeout(
            REDIS_CONNECT_TIMEOUT,
            self.client.get_multiplexed_async_connection(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Redis connection timed out"))?
        .map_err(|e| anyhow::anyhow!("Redis connection failed: {}", e))
    }

    /// Enqueue a new ML job for first-time processing.
    pub async fn enqueue_ml_job(&self, job: MlJob) -> anyhow::Result<()> {
        let mut conn = self.connection().await?;
        let _: usize = conn.lpush(ML_JOBS_KEY, serde_json::to_string(&job)?).await?;
        Ok(())
    }

    /// Re-enqueue a job that failed, with its incremented `retry_count` already set.
    pub async fn requeue_ml_job(&self, job: MlJob) -> anyhow::Result<()> {
        let mut conn = self.connection().await?;
        let _: usize = conn.lpush(ML_JOBS_KEY, serde_json::to_string(&job)?).await?;
        Ok(())
    }

    /// Block-pop a job from the queue. Returns `None` on timeout (no items within 5 s).
    pub async fn dequeue_ml_job(&self) -> anyhow::Result<Option<MlJob>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let res: Option<(String, String)> = conn.brpop(ML_JOBS_KEY, 5.0).await?;
        match res {
            Some((_, json)) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Move a permanently-failed job to the dead-letter queue for manual inspection.
    pub async fn enqueue_to_dlq(&self, job: &MlJob) -> anyhow::Result<()> {
        let mut conn = self.connection().await?;
        let _: usize = conn
            .lpush(ML_JOBS_DLQ_KEY, serde_json::to_string(job)?)
            .await?;
        Ok(())
    }
}
