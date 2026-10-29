use crate::limits::{RateLimitPolicy, SecurityLimits};
use redis::{
    Client, RedisError,
    aio::{ConnectionManager, ConnectionManagerConfig},
};
use std::{io, sync::Arc, time::Duration};
use tokio::sync::{OnceCell, RwLock};

const ACQUIRE: &str = r#"
    local current = tonumber(redis.call('GET', KEYS[1]) or '0')
    if redis.call('TTL', KEYS[1]) == -1 then
        redis.call('EXPIRE', KEYS[1], ARGV[2])
    end
    if current >= tonumber(ARGV[1]) then return 0 end
    local count = redis.call('INCR', KEYS[1])
    if count == 1 then redis.call('EXPIRE', KEYS[1], ARGV[2]) end
    return 1
"#;

/// Shared abuse-control connection. Backend errors never grant permission.
pub struct RateLimiter {
    client: Client,
    // Each generation shares initialization without holding this lock during I/O.
    connection: RwLock<Arc<OnceCell<ConnectionManager>>>,
    connect_timeout: Duration,
    response_timeout: Duration,
}

impl RateLimiter {
    pub fn new(client: Client, limits: &SecurityLimits) -> Self {
        Self {
            client,
            connection: RwLock::new(Arc::new(OnceCell::new())),
            connect_timeout: Duration::from_millis(limits.redis_connect_timeout_ms.into()),
            response_timeout: Duration::from_millis(limits.redis_response_timeout_ms.into()),
        }
    }

    async fn connection(
        &self,
        slot: &OnceCell<ConnectionManager>,
    ) -> Result<ConnectionManager, RedisError> {
        slot.get_or_try_init(|| async {
            ConnectionManager::new_with_config(
                self.client.clone(),
                ConnectionManagerConfig::new()
                    .set_connection_timeout(Some(self.connect_timeout))
                    .set_response_timeout(Some(self.response_timeout))
                    .set_number_of_retries(0),
            )
            .await
        })
        .await
        .cloned()
    }

    /// Atomically claim a slot in a window starting at the first counted attempt.
    ///
    /// Returns `false` for an exhausted limit, or an error if enforcement is unavailable.
    /// The deadline includes waiting for another request to initialize the connection.
    pub async fn check(&self, key: &str, policy: &RateLimitPolicy) -> Result<bool, RedisError> {
        if policy.requests == 0 || policy.window_seconds == 0 {
            return Err(RedisError::from((
                redis::ErrorKind::InvalidClientConfig,
                "Rate limits must be positive",
            )));
        }
        let slot = self.connection.read().await.clone();
        let result = tokio::time::timeout(self.connect_timeout + self.response_timeout, async {
            let mut connection = self.connection(&slot).await?;
            // EVAL avoids extra SCRIPT LOAD/EVALSHA requests after provider cache resets.
            redis::cmd("EVAL")
                .arg(ACQUIRE)
                .arg(1)
                .arg(format!("abuse:{key}"))
                .arg(policy.requests)
                .arg(policy.window_seconds)
                .query_async(&mut connection)
                .await
        })
        .await
        .unwrap_or_else(|_| {
            Err(RedisError::from(io::Error::new(
                io::ErrorKind::TimedOut,
                "Rate limit backend deadline exceeded",
            )))
        });
        if result.as_ref().is_err_and(|error| {
            error.is_timeout() || error.is_io_error() || error.is_unrecoverable_error()
        }) {
            let mut current = self.connection.write().await;
            // An older failed request must not discard a replacement connection.
            if Arc::ptr_eq(&current, &slot) {
                *current = Arc::new(OnceCell::new());
            }
        }
        // Never retry an uncertain EVAL: its quota claim may already have executed.
        result
    }
}

#[cfg(test)]
mod tests;
