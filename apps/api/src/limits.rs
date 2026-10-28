use anyhow::{Context, Result, ensure};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub redis_budget: RedisBudget,
    pub cache: CacheLimits,
    pub queue: QueueLimits,
    pub gallery: GalleryLimits,
    pub uploads_per_ip_per_day: u32,
    pub web: WebLimits,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedisBudget {
    pub monthly_commands: u64,
    pub reserved_commands: u64,
    pub billing_days: u64,
    pub worker_instances: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheLimits {
    pub ttl_seconds: u64,
    pub max_entries: usize,
    pub max_bytes: usize,
    pub max_value_bytes: usize,
    pub fetch_lanes: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueLimits {
    pub block_seconds: u64,
    pub connect_timeout_seconds: u64,
    pub response_grace_seconds: u64,
    pub retry_initial_seconds: u64,
    pub retry_max_seconds: u64,
    pub job_max_attempts: u32,
    pub http_timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GalleryLimits {
    pub default_limit: i64,
    pub max_limit: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebLimits {
    pub dev_port: u16,
    pub live_dev_port: u16,
    pub gallery_page_size: i64,
    pub query_stale_seconds: u64,
    pub query_retries: u32,
    pub image_cache_entries: usize,
    pub image_cache_seconds: u64,
}

impl Limits {
    pub fn load() -> Result<Self> {
        match std::env::var_os("TTL_LIMITS_FILE") {
            Some(path) => {
                let text = std::fs::read_to_string(path).context("read TTL_LIMITS_FILE")?;
                Self::parse(&text)
            }
            None => Self::bundled(),
        }
    }

    pub fn bundled() -> Result<Self> {
        Self::parse(include_str!("../config/runtime.json"))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let limits: Self = serde_json::from_str(text).context("parse runtime limits")?;
        limits.validate()?;
        Ok(limits)
    }

    pub fn idle_queue_commands(&self) -> Result<u64> {
        let seconds = self
            .redis_budget
            .billing_days
            .checked_mul(86_400)
            .context("billing period overflow")?;
        ensure!(
            self.queue.block_seconds > 0,
            "queue.block_seconds must be positive"
        );
        seconds
            .div_ceil(self.queue.block_seconds)
            .checked_mul(self.redis_budget.worker_instances)
            .context("worker budget overflow")
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.web.dev_port > 0
                && self.web.live_dev_port > 0
                && self.web.dev_port != self.web.live_dev_port,
            "local and live ports must be nonzero and distinct"
        );
        ensure!(
            (1..=31).contains(&self.redis_budget.billing_days),
            "billing_days must be 1..31"
        );
        ensure!(
            self.redis_budget.worker_instances > 0,
            "worker_instances must be positive"
        );
        ensure!(
            self.cache.fetch_lanes <= self.cache.max_entries,
            "fetch lanes exceed entry capacity"
        );
        ensure!(
            std::time::Instant::now()
                .checked_add(std::time::Duration::from_secs(self.cache.ttl_seconds))
                .is_some(),
            "cache TTL exceeds clock range"
        );
        ensure!(
            self.cache.fetch_lanes > 0,
            "cache.fetch_lanes must be positive"
        );
        ensure!(
            self.cache.max_entries > 0
                && self.cache.max_bytes > 0
                && self.cache.max_value_bytes > 0,
            "cache capacities must be positive"
        );
        ensure!(
            self.cache.max_value_bytes <= self.cache.max_bytes,
            "cache value limit exceeds capacity"
        );
        ensure!(
            self.queue.connect_timeout_seconds > 0 && self.queue.response_grace_seconds > 0,
            "queue timeouts must be positive"
        );
        ensure!(
            self.queue
                .block_seconds
                .checked_add(self.queue.response_grace_seconds)
                .is_some(),
            "queue response timeout overflow"
        );
        ensure!(
            self.queue.retry_initial_seconds > 0
                && self.queue.retry_max_seconds >= self.queue.retry_initial_seconds,
            "queue retry bounds are invalid"
        );
        ensure!(
            self.queue.job_max_attempts > 0 && self.queue.http_timeout_seconds > 0,
            "worker limits must be positive"
        );
        ensure!(
            self.gallery.default_limit > 0 && self.gallery.max_limit >= self.gallery.default_limit,
            "gallery limits are invalid"
        );
        ensure!(
            self.web.gallery_page_size > 0 && self.web.gallery_page_size <= self.gallery.max_limit,
            "web page size exceeds gallery limit"
        );
        ensure!(
            self.web.image_cache_entries > 0 && self.web.image_cache_seconds > 0,
            "image cache limits must be positive"
        );
        let planned = self
            .idle_queue_commands()?
            .checked_add(self.redis_budget.reserved_commands)
            .context("monthly budget overflow")?;
        ensure!(
            planned <= self.redis_budget.monthly_commands,
            "idle workers plus reserve exceed monthly Redis budget"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> serde_json::Value {
        serde_json::from_str(include_str!("../config/runtime.json")).unwrap()
    }

    #[test]
    fn bundled_budget_leaves_reserve() {
        let mut limits = Limits::bundled().unwrap();
        limits.queue.block_seconds = 300;
        limits.redis_budget.billing_days = 31;
        limits.redis_budget.worker_instances = 1;
        assert_eq!(limits.idle_queue_commands().unwrap(), 8928);
    }

    #[test]
    fn rejects_zero_wait_unknown_keys_and_over_budget_workers() {
        for (section, key, value) in [
            ("queue", "block_seconds", 0),
            ("redis_budget", "worker_instances", 100),
            ("cache", "fetch_lanes", 0),
        ] {
            let mut data = settings();
            data[section][key] = value.into();
            assert!(Limits::parse(&data.to_string()).is_err());
        }
        let mut data = settings();
        data["queue"]["block_second"] = 20.into();
        assert!(Limits::parse(&data.to_string()).is_err());
    }

    #[test]
    fn rejects_shared_development_ports() {
        let mut data = settings();
        data["web"]["live_dev_port"] = data["web"]["dev_port"].clone();
        assert!(Limits::parse(&data.to_string()).is_err());
    }

    #[test]
    fn rejects_invalid_capacity_page_size_and_overflow() {
        let mut data = settings();
        data["cache"]["max_bytes"] = 1.into();
        assert!(Limits::parse(&data.to_string()).is_err());
        let mut data = settings();
        data["web"]["gallery_page_size"] = 1000.into();
        assert!(Limits::parse(&data.to_string()).is_err());
        let mut data = settings();
        data["redis_budget"]["worker_instances"] = u64::MAX.into();
        assert!(Limits::parse(&data.to_string()).is_err());
    }
}
