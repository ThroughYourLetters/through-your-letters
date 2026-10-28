use crate::limits::QueueLimits;
use redis::aio::{ConnectionManager, ConnectionManagerConfig, MultiplexedConnection};
use redis::{AsyncCommands, AsyncConnectionConfig, Client};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::{Mutex, OnceCell};
use uuid::Uuid;

const ML_JOBS_KEY: &str = "ml_jobs";
const ML_JOBS_DLQ_KEY: &str = "ml_jobs_dlq";

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
    limits: QueueLimits,
    producer: OnceCell<ConnectionManager>,
    // BRPOP must never share a connection with LPUSH: it blocks the stream.
    consumer: Mutex<Option<MultiplexedConnection>>,
}

impl RedisQueue {
    pub fn new(client: Client, limits: QueueLimits) -> Self {
        Self {
            client,
            limits,
            producer: OnceCell::new(),
            consumer: Mutex::new(None),
        }
    }

    pub fn limits(&self) -> &QueueLimits {
        &self.limits
    }

    async fn connection(&self) -> anyhow::Result<ConnectionManager> {
        let manager = self
            .producer
            .get_or_try_init(|| async {
                ConnectionManager::new_with_config(
                    self.client.clone(),
                    ConnectionManagerConfig::new()
                        .set_connection_timeout(Some(Duration::from_secs(
                            self.limits.connect_timeout_seconds,
                        )))
                        .set_response_timeout(Some(Duration::from_secs(
                            self.limits.connect_timeout_seconds,
                        )))
                        .set_number_of_retries(0),
                )
                .await
            })
            .await?;
        Ok(manager.clone())
    }

    /// Enqueue a new ML job for first-time processing.
    pub async fn enqueue_ml_job(&self, job: MlJob) -> anyhow::Result<()> {
        let mut conn = self.connection().await?;
        let _: usize = conn
            .lpush(ML_JOBS_KEY, serde_json::to_string(&job)?)
            .await?;
        Ok(())
    }

    /// Re-enqueue a job that failed, with its incremented `retry_count` already set.
    pub async fn requeue_ml_job(&self, job: MlJob) -> anyhow::Result<()> {
        let mut conn = self.connection().await?;
        let _: usize = conn
            .lpush(ML_JOBS_KEY, serde_json::to_string(&job)?)
            .await?;
        Ok(())
    }

    /// Reuse a separate blocking consumer connection.
    /// An arriving job wakes BRPOP immediately; the timeout adds no job latency.
    pub async fn dequeue_ml_job(&self) -> anyhow::Result<Option<MlJob>> {
        let mut consumer = self.consumer.lock().await;
        if consumer.is_none() {
            let config = AsyncConnectionConfig::new()
                .set_connection_timeout(Some(Duration::from_secs(
                    self.limits.connect_timeout_seconds,
                )))
                .set_response_timeout(Some(Duration::from_secs(
                    self.limits.block_seconds + self.limits.response_grace_seconds,
                )));
            *consumer = Some(
                self.client
                    .get_multiplexed_async_connection_with_config(&config)
                    .await?,
            );
        }
        let res: redis::RedisResult<Option<(String, String)>> = consumer
            .as_mut()
            .expect("consumer initialized")
            .brpop(ML_JOBS_KEY, self.limits.block_seconds as f64)
            .await;
        match res {
            Ok(Some((_, json))) => Ok(Some(serde_json::from_str(&json)?)),
            Ok(None) => Ok(None),
            Err(error) => {
                *consumer = None; // Reconnect only after the worker's backoff.
                Err(error.into())
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::{
        io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
        sync::Notify,
    };

    // A local RESP fixture counts connections and verifies the actual wire
    // commands without credentials, an Upstash account, or idle-minute waits.
    fn limits() -> crate::limits::QueueLimits {
        let mut limits = crate::limits::Limits::bundled().unwrap().queue;
        limits.block_seconds = 17;
        limits
    }

    async fn server() -> (
        Client,
        Arc<AtomicUsize>,
        Arc<Notify>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("redis://{}/", listener.local_addr().unwrap());
        let connections = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Notify::new());
        let count = connections.clone();
        let waiting = started.clone();
        let pending = Arc::new(Mutex::new(std::collections::VecDeque::<String>::new()));
        let available = Arc::new(Notify::new());
        let task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                count.fetch_add(1, Ordering::SeqCst);
                let waiting = waiting.clone();
                let pending = pending.clone();
                let available = available.clone();
                tokio::spawn(async move {
                    let mut stream = BufReader::new(stream);
                    loop {
                        let mut line = String::new();
                        if stream.read_line(&mut line).await.unwrap_or(0) == 0 {
                            break;
                        }
                        let size: usize = line.trim().strip_prefix('*').unwrap().parse().unwrap();
                        let mut parts = Vec::new();
                        for _ in 0..size {
                            line.clear();
                            stream.read_line(&mut line).await.unwrap();
                            let len: usize =
                                line.trim().strip_prefix('$').unwrap().parse().unwrap();
                            let mut bytes = vec![0; len + 2];
                            stream.read_exact(&mut bytes).await.unwrap();
                            parts.push(String::from_utf8(bytes[..len].to_vec()).unwrap());
                        }
                        let response = match parts[0].as_str() {
                            "BRPOP" => {
                                assert_eq!(parts[2].parse::<f64>().unwrap(), 17.0);
                                waiting.notify_one();
                                let _ = tokio::time::timeout(
                                    Duration::from_millis(30),
                                    available.notified(),
                                )
                                .await;
                                if let Some(job) = pending.lock().await.pop_front() {
                                    format!("*2\r\n$7\r\nml_jobs\r\n${}\r\n{}\r\n", job.len(), job)
                                } else {
                                    "$-1\r\n".to_owned()
                                }
                            }
                            "LPUSH" => {
                                if parts[1] == "ml_jobs" {
                                    pending.lock().await.push_back(parts[2].clone());
                                    available.notify_one();
                                }
                                ":1\r\n".to_owned()
                            }
                            "PING" => "+PONG\r\n".to_owned(),
                            _ => "+OK\r\n".to_owned(),
                        };
                        if stream
                            .get_mut()
                            .write_all(response.as_bytes())
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                });
            }
        });
        (Client::open(url).unwrap(), connections, started, task)
    }

    #[tokio::test]
    async fn idle_pops_reuse_one_connection_and_use_long_block_timeout() {
        let (client, connections, _, server) = server().await;
        let queue = RedisQueue::new(client, limits());
        for _ in 0..3 {
            assert!(queue.dequeue_ml_job().await.unwrap().is_none());
        }
        assert_eq!(connections.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test]
    async fn producers_use_a_separate_reusable_connection() {
        let (client, connections, started, server) = server().await;
        let queue = Arc::new(RedisQueue::new(client, limits()));
        let consumer = queue.clone();
        let task = tokio::spawn(async move { consumer.dequeue_ml_job().await });
        tokio::time::timeout(Duration::from_secs(2), started.notified())
            .await
            .unwrap();
        let job = MlJob {
            lettering_id: Uuid::now_v7(),
            image_url: "https://example.test/image.webp".into(),
            retry_count: 0,
        };
        queue.enqueue_ml_job(job.clone()).await.unwrap();
        queue.requeue_ml_job(job.clone()).await.unwrap();
        queue.enqueue_to_dlq(&job).await.unwrap();
        let popped = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(popped.lettering_id, job.lettering_id);
        assert_eq!(connections.load(Ordering::SeqCst), 2);
        server.abort();
    }
}
