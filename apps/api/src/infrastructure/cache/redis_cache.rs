use anyhow::Result;
use redis::{AsyncCommands, Client};
use serde::{Serialize, de::DeserializeOwned};
use std::future::Future;
use tracing::{debug, error, warn};

/// Lock TTL in seconds. Short-lived to avoid deadlocks if the holder crashes.
const LOCK_TTL_SECONDS: u64 = 10;

/// How long to wait between retries when another request holds the lock (ms).
const LOCK_RETRY_INTERVAL_MS: u64 = 50;

/// Maximum number of retries waiting for another request to populate cache.
const LOCK_MAX_RETRIES: u32 = 60; // 60 * 50ms = 3 seconds max wait

/// Extra TTL added to stale data beyond the main TTL, enabling stale-while-revalidate.
const STALE_EXTENSION_SECONDS: u64 = 60;

pub struct RedisCache {
    client: Client,
}

impl RedisCache {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let value: Option<String> = conn.get(key).await?;
        match value {
            Some(v) => Ok(Some(serde_json::from_str(&v)?)),
            None => Ok(None),
        }
    }

    pub async fn set<T: Serialize>(&self, key: &str, value: &T, ttl: u64) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let json = serde_json::to_string(value)?;
        let _: () = conn.set_ex(key, json, ttl).await?;
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let _: () = conn.del(key).await?;
        Ok(())
    }

    /// Fetch-through cache with stampede protection.
    ///
    /// On cache miss, only one request fetches from the source (lock winner).
    /// Other concurrent requests wait for the cache to be populated.
    ///
    /// Error philosophy: Redis failures are *degraded*, not fatal. If Redis
    /// is completely down, we skip caching and go straight to the source.
    /// But we log every failure loudly so operators know Redis needs attention.
    /// The fetch_fn errors, on the other hand, are *always* propagated — those
    /// represent the actual business logic failing.
    pub async fn get_or_fetch<T, F, Fut>(&self, key: &str, ttl: u64, fetch_fn: F) -> Result<T>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        // Step 1: Try fresh cache. If Redis is down, log and skip to fetch.
        match self.get::<T>(key).await {
            Ok(Some(cached)) => {
                debug!("Cache HIT for key={}", key);
                return Ok(cached);
            }
            Ok(None) => {
                debug!("Cache MISS for key={}", key);
            }
            Err(e) => {
                // Redis is unhealthy. Don't silently pretend it's a cache miss —
                // log loudly and go straight to the source. No point trying to
                // acquire a lock on a broken Redis.
                error!(
                    "Redis GET failed for key={}: {}. Bypassing cache entirely.",
                    key, e
                );
                return fetch_fn().await;
            }
        }

        let stale_key = format!("{}:stale", key);
        let lock_key = format!("{}:lock", key);

        // Step 2: Try to acquire lock (SET NX EX — atomic, Upstash-safe)
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(c) => c,
            Err(e) => {
                error!(
                    "Redis connection failed for lock on key={}: {}. Fetching directly.",
                    key, e
                );
                return fetch_fn().await;
            }
        };

        let lock_acquired: bool = redis::cmd("SET")
            .arg(&lock_key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(LOCK_TTL_SECONDS)
            .query_async::<Option<String>>(&mut conn)
            .await
            .map(|v| v.is_some())
            .unwrap_or_else(|e| {
                error!(
                    "Redis SET NX failed for lock key={}: {}. Treating as lock-not-acquired.",
                    lock_key, e
                );
                false
            });

        if lock_acquired {
            debug!("Lock acquired for key={}, fetching from source", key);
            let result = fetch_fn().await;

            if let Ok(value) = &result {
                // Cache write failures are logged but don't fail the request.
                // The fetch succeeded — the caller gets their data regardless.
                // But we make noise so operators know the cache isn't working.
                if let Err(e) = self.set(key, value, ttl).await {
                    error!(
                        "Failed to write cache for key={}: {}. Response served but NOT cached — next request will hit DB again.",
                        key, e
                    );
                }
                if let Err(e) = self
                    .set(&stale_key, value, ttl + STALE_EXTENSION_SECONDS)
                    .await
                {
                    warn!(
                        "Failed to write stale cache for key={}: {}. Stale-while-revalidate won't work for this key.",
                        key, e
                    );
                }
            }

            // Release lock. If this fails, the lock will auto-expire in LOCK_TTL_SECONDS.
            // That's acceptable but means other waiters block for up to 10s instead of
            // being unblocked immediately.
            if let Err(e) = conn.del::<_, ()>(&lock_key).await {
                warn!(
                    "Failed to release lock for key={}: {}. Lock will auto-expire in {}s.",
                    key, e, LOCK_TTL_SECONDS
                );
            }

            return result;
        }

        // Step 3: Lock NOT acquired — another request is populating. Wait and retry.
        debug!("Lock held by another request for key={}, waiting", key);
        for attempt in 0..LOCK_MAX_RETRIES {
            tokio::time::sleep(std::time::Duration::from_millis(LOCK_RETRY_INTERVAL_MS)).await;
            match self.get::<T>(key).await {
                Ok(Some(cached)) => {
                    debug!(
                        "Cache populated by peer on attempt {} for key={}",
                        attempt, key
                    );
                    return Ok(cached);
                }
                Ok(None) => continue, // Not populated yet, keep waiting
                Err(e) => {
                    // Redis broke while we were waiting. Stop waiting and try stale/direct.
                    error!(
                        "Redis failed during wait loop for key={} (attempt {}): {}. Breaking out.",
                        key, attempt, e
                    );
                    break;
                }
            }
        }

        // Step 4: Retries exhausted or Redis failed. Try stale data.
        match self.get::<T>(&stale_key).await {
            Ok(Some(stale)) => {
                warn!("Serving STALE data for key={}", key);
                return Ok(stale);
            }
            Ok(None) => {
                debug!("No stale data available for key={}", key);
            }
            Err(e) => {
                error!("Redis failed reading stale key={}: {}", stale_key, e);
            }
        }

        // Step 5: No cache, no stale data, Redis possibly broken. Direct fetch.
        // This is the last resort and we log at error level because it means
        // the stampede protection isn't working.
        error!(
            "Cache stampede protection failed for key={}. Fetching directly from source. \
             This request is unprotected — if many requests hit this path simultaneously, \
             the database will see all of them.",
            key
        );
        fetch_fn().await
    }
}
