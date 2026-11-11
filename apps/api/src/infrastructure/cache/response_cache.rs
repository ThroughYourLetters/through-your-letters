//! Bounded, per-process public response cache. Never sends Redis commands.
//!
//! Short TTLs bound cross-replica staleness. Writes invalidate this instance;
//! generation checks keep an in-flight pre-write fetch from repopulating it.
use crate::limits::CacheLimits;
use anyhow::Result;
use serde::Serialize;
use std::{
    any::Any,
    collections::HashMap,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

struct Entry {
    value: Arc<dyn Any + Send + Sync>,
    weight: usize,
    expires: Instant,
    inserted: Instant,
}

#[derive(Default)]
struct State {
    entries: HashMap<String, Entry>,
    bytes: usize,
    generation: u64,
}

impl State {
    fn remove(&mut self, key: &str) {
        if let Some(entry) = self.entries.remove(key) {
            self.bytes -= entry.weight;
        }
    }
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

    async fn get<T: Clone + Send + Sync + 'static>(&self, key: &str) -> Option<T> {
        let mut state = self.state.lock().await;
        if let Some(entry) = state.entries.get(key) {
            if entry.expires > Instant::now() && entry.value.is::<T>() {
                let value = entry.value.clone();
                drop(state);
                return value.downcast_ref::<T>().cloned();
            }
            state.remove(key);
        }
        None
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
        T: Serialize + Clone + Send + Sync + 'static,
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        if let Some(value) = self.get(key).await {
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
        if let Some(value) = self.get(key).await {
            tracing::debug!(cache = "public-memory", outcome = "coalesced");
            return Ok(value);
        }
        let generation = self.state.lock().await.generation;
        let value = fetch().await?; // Failed loads are never cached.
        let ttl = Duration::from_secs(ttl).min(self.max_ttl);
        if ttl.is_zero() || self.max_entries == 0 {
            return Ok(value);
        }
        // Keep the existing serialized-size budget without retaining a second copy.
        let value_bytes = serde_json::to_vec(&value)?.len();
        let weight = key.len().saturating_add(value_bytes);
        if value_bytes <= self.max_value_bytes && weight <= self.max_bytes {
            let mut state = self.state.lock().await;
            if generation == state.generation {
                state.remove(key);
                // Remove expired entries first; then evict oldest until both caps fit.
                let now = Instant::now();
                state.entries.retain(|_, entry| entry.expires > now);
                state.bytes = state.entries.values().map(|entry| entry.weight).sum();
                while state.entries.len() >= self.max_entries
                    || state.bytes + weight > self.max_bytes
                {
                    let oldest = state
                        .entries
                        .iter()
                        .min_by_key(|(_, e)| e.inserted)
                        .map(|(k, _)| k.clone());
                    if let Some(oldest) = oldest {
                        state.remove(&oldest);
                    } else {
                        break;
                    }
                }
                state.bytes += weight;
                state.entries.insert(
                    key.to_owned(),
                    Entry {
                        value: Arc::new(value.clone()),
                        weight,
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
    async fn hits_clone_the_response_without_serializing_or_deserializing_it() {
        #[derive(Clone)]
        struct Counted(Arc<AtomicUsize>, u32);
        impl Serialize for Counted {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.0.fetch_add(1, Ordering::SeqCst);
                serializer.serialize_u32(self.1)
            }
        }
        let cache = cache();
        let serializations = Arc::new(AtomicUsize::new(0));
        for _ in 0..100 {
            let response = cache
                .get_or_fetch("typed", 15, || async {
                    Ok(Counted(serializations.clone(), 42))
                })
                .await
                .unwrap();
            assert_eq!(response.1, 42);
        }
        assert_eq!(serializations.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn reusing_a_key_for_another_type_reloads_without_corrupting_the_budget() {
        let cache = cache();
        cache
            .get_or_fetch("same", 15, || async { Ok(42_u32) })
            .await
            .unwrap();
        assert_eq!(
            cache
                .get_or_fetch("same", 15, || async { Ok("fresh".to_string()) })
                .await
                .unwrap(),
            "fresh"
        );
        let state = cache.state.lock().await;
        assert_eq!(state.entries.len(), 1);
        assert_eq!(
            state.bytes,
            "same".len() + serde_json::to_vec("fresh").unwrap().len()
        );
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
