//! Bounded, per-process public response cache. Never sends Redis commands.
//!
//! Short TTLs bound cross-replica staleness. Writes invalidate this instance;
//! generation checks keep an in-flight pre-write fetch from repopulating it.
use crate::limits::CacheLimits;
use anyhow::Result;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::HashMap,
    future::Future,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

struct Entry {
    bytes: Vec<u8>,
    expires: Instant,
    inserted: Instant,
}

#[derive(Default)]
struct State {
    entries: HashMap<String, Entry>,
    bytes: usize,
    generation: u64,
}

pub struct ResponseCache {
    state: Mutex<State>,
    // A fixed number of locks bounds memory even for arbitrary filter keys.
    lanes: Vec<Mutex<()>>,
    max_entries: usize,
    max_bytes: usize,
    max_value_bytes: usize,
    max_ttl: Duration,
}

impl ResponseCache {
    pub fn new(limits: CacheLimits) -> Self {
        Self {
            state: Mutex::new(State::default()),
            lanes: (0..limits.fetch_lanes).map(|_| Mutex::new(())).collect(),
            max_entries: limits.max_entries,
            max_bytes: limits.max_bytes,
            max_value_bytes: limits.max_value_bytes,
            max_ttl: Duration::from_secs(limits.ttl_seconds),
        }
    }

    async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let mut state = self.state.lock().await;
        if let Some(entry) = state.entries.get(key) {
            if entry.expires > Instant::now() {
                return Ok(Some(serde_json::from_slice(&entry.bytes)?));
            }
            let removed = state.entries.remove(key).expect("entry exists");
            state.bytes -= removed.bytes.len() + key.len();
        }
        Ok(None)
    }

    pub async fn invalidate(&self) {
        let mut state = self.state.lock().await;
        state.generation = state.generation.wrapping_add(1);
        state.entries.clear();
        state.bytes = 0;
        tracing::debug!(cache = "public-memory", outcome = "invalidated");
    }

    pub async fn get_or_fetch<T, F, Fut>(&self, key: &str, ttl: u64, fetch: F) -> Result<T>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        if let Some(value) = self.get(key).await? {
            tracing::debug!(cache = "public-memory", outcome = "hit");
            return Ok(value);
        }
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hash);
        let _lane = self.lanes[hash.finish() as usize % self.lanes.len()]
            .lock()
            .await;
        // Same-key waiters reuse the result locally, with no polling/network I/O.
        if let Some(value) = self.get(key).await? {
            tracing::debug!(cache = "public-memory", outcome = "coalesced");
            return Ok(value);
        }
        let generation = self.state.lock().await.generation;
        let value = fetch().await?; // Failed loads are never cached.
        let bytes = serde_json::to_vec(&value)?;
        let ttl = Duration::from_secs(ttl).min(self.max_ttl);
        if !ttl.is_zero()
            && bytes.len() <= self.max_value_bytes
            && bytes.len() + key.len() <= self.max_bytes
            && self.max_entries > 0
        {
            let mut state = self.state.lock().await;
            if generation == state.generation {
                // Remove expired entries first; then evict oldest until both caps fit.
                let now = Instant::now();
                state.entries.retain(|_, entry| entry.expires > now);
                state.bytes = state
                    .entries
                    .iter()
                    .map(|(key, e)| key.len() + e.bytes.len())
                    .sum();
                while state.entries.len() >= self.max_entries
                    || state.bytes + key.len() + bytes.len() > self.max_bytes
                {
                    let oldest = state
                        .entries
                        .iter()
                        .min_by_key(|(_, e)| e.inserted)
                        .map(|(k, _)| k.clone());
                    if let Some(oldest) = oldest {
                        let entry = state.entries.remove(&oldest).expect("entry exists");
                        state.bytes -= oldest.len() + entry.bytes.len();
                    } else {
                        break;
                    }
                }
                state.bytes += key.len() + bytes.len();
                state.entries.insert(
                    key.to_owned(),
                    Entry {
                        bytes,
                        expires: now + ttl,
                        inserted: now,
                    },
                );
            }
        }
        tracing::debug!(cache = "public-memory", outcome = "load");
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn cache() -> ResponseCache {
        ResponseCache::new(crate::limits::Limits::bundled().unwrap().cache)
    }

    #[tokio::test]
    async fn concurrent_readers_share_one_load() {
        let cache = Arc::new(cache());
        let loads = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..100 {
            let cache = cache.clone();
            let loads = loads.clone();
            tasks.push(tokio::spawn(async move {
                cache
                    .get_or_fetch("gallery", 15, || async {
                        loads.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        Ok(42_u32)
                    })
                    .await
                    .unwrap()
            }));
        }
        for task in tasks {
            assert_eq!(task.await.unwrap(), 42);
        }
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failures_are_not_cached() {
        let cache = cache();
        assert!(
            cache
                .get_or_fetch::<u32, _, _>("g", 15, || async { anyhow::bail!("DB failed") })
                .await
                .is_err()
        );
        assert_eq!(
            cache
                .get_or_fetch("g", 15, || async { Ok(7_u32) })
                .await
                .unwrap(),
            7
        );
    }

    #[tokio::test]
    async fn expired_and_invalidated_entries_reload() {
        let cache = cache();
        cache
            .get_or_fetch("g", 15, || async { Ok(1_u32) })
            .await
            .unwrap();
        cache
            .state
            .lock()
            .await
            .entries
            .get_mut("g")
            .unwrap()
            .expires = Instant::now();
        assert_eq!(
            cache
                .get_or_fetch("g", 15, || async { Ok(2_u32) })
                .await
                .unwrap(),
            2
        );
        cache.invalidate().await;
        assert_eq!(
            cache
                .get_or_fetch("g", 15, || async { Ok(3_u32) })
                .await
                .unwrap(),
            3
        );
    }

    #[tokio::test]
    async fn invalidation_during_load_cannot_repopulate_old_data() {
        let cache = cache();
        let value = cache
            .get_or_fetch("g", 15, || async {
                cache.invalidate().await;
                Ok(1_u32)
            })
            .await
            .unwrap();
        assert_eq!(value, 1);
        assert!(cache.state.lock().await.entries.is_empty());
        assert_eq!(
            cache
                .get_or_fetch("g", 15, || async { Ok(2_u32) })
                .await
                .unwrap(),
            2
        );
    }

    #[tokio::test]
    async fn bounds_entries_and_bytes_and_skips_oversized_values() {
        let cache = ResponseCache::new(CacheLimits {
            max_entries: 2,
            max_bytes: 30,
            max_value_bytes: 20,
            ..crate::limits::Limits::bundled().unwrap().cache
        });
        for key in ["a", "b", "c"] {
            cache
                .get_or_fetch(key, 15, || async { Ok("abcdefghij".to_string()) })
                .await
                .unwrap();
        }
        let state = cache.state.lock().await;
        assert_eq!(state.entries.len(), 2);
        assert!(state.bytes <= 30);
        drop(state);
        cache
            .get_or_fetch("huge", 15, || async {
                Ok("x".repeat(cache.max_value_bytes + 1))
            })
            .await
            .unwrap();
        assert!(!cache.state.lock().await.entries.contains_key("huge"));
    }

    #[tokio::test]
    async fn ttl_is_capped_and_zero_disables_storage() {
        let cache = cache();
        cache
            .get_or_fetch("g", 9999, || async { Ok(1_u32) })
            .await
            .unwrap();
        assert!(cache.state.lock().await.entries["g"].expires <= Instant::now() + cache.max_ttl);
        cache
            .get_or_fetch("none", 0, || async { Ok(2_u32) })
            .await
            .unwrap();
        assert!(!cache.state.lock().await.entries.contains_key("none"));
    }
}
