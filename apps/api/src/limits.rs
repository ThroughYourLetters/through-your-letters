use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::num::NonZeroUsize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub redis_budget: RedisBudget,
    pub cache: CacheLimits,
    pub queue: QueueLimits,
    pub gallery: GalleryLimits,
    pub map: MapLimits,
    pub location_repairs: LocationRepairLimits,
    pub geocoding: GeocodingLimits,
    pub email: EmailLimits,
    pub security: SecurityLimits,
    pub passwords: PasswordLimits,
    pub images: ImageLimits,
    pub uploads_per_ip_per_day: u32,
    pub web: WebLimits,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordLimits {
    pub workers: NonZeroUsize,
    pub work_timeout_ms: u64,
    pub max_verify_bytes: usize,
    pub max_verify_cost: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageLimits {
    pub workers: NonZeroUsize,
    pub concurrent_uploads: NonZeroUsize,
    pub max_input_bytes: usize,
    pub max_pixels: u64,
    pub max_dimension: u32,
    pub max_decoded_bytes: u64,
    pub work_timeout_ms: u64,
    pub upload_timeout_ms: u64,
    pub scan_timeout_ms: u64,
    pub max_field_bytes: usize,
    pub max_fields: usize,
    pub max_provider_response_bytes: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedisBudget {
    pub monthly_commands: u64,
    pub reserved_commands: u64,
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
    pub poll_seconds: u64,
    pub lease_seconds: u64,
    pub job_timeout_seconds: u64,
    pub recovery_seconds: u64,
    pub recovery_batch_size: u32,
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
pub struct LocationRepairLimits {
    pub max_evidence_bytes: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapLimits {
    pub max_cells: u32,
    pub grid_pixels: u32,
    pub query_timeout_ms: u32,
    pub move_debounce_ms: u32,
    pub query_gc_seconds: u32,
    pub heat_radius_pixels: u32,
    pub heat_blur_pixels: u32,
    pub heat_saturation_count: u32,
    pub detail_zoom: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeocodingLimits {
    pub search_url: String,
    pub request_timeout_ms: u32,
    pub total_timeout_ms: u32,
    pub min_interval_ms: u32,
    pub failure_cooldown_seconds: u32,
    pub positive_ttl_seconds: u32,
    pub negative_ttl_seconds: u32,
    pub max_entries: u32,
    pub max_response_bytes: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmailLimits {
    pub request_timeout_ms: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitPolicy {
    pub requests: u32,
    pub window_seconds: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityLimits {
    pub redis_connect_timeout_ms: u32,
    pub redis_response_timeout_ms: u32,
    pub max_forwarded_hops: usize,
    pub register: RateLimitPolicy,
    pub login: RateLimitPolicy,
    pub forgot_password: RateLimitPolicy,
    pub reset_password: RateLimitPolicy,
    pub admin_login: RateLimitPolicy,
    pub comment: RateLimitPolicy,
    pub report: RateLimitPolicy,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebLimits {
    pub geolocation_timeout_seconds: u32,
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

    fn validate(&self) -> Result<()> {
        let images = &self.images;
        ensure!(
            images.workers.get() <= 4
                && images.concurrent_uploads.get() <= 8
                && (1024..=33_554_432).contains(&images.max_input_bytes)
                && (1..=64_000_000).contains(&images.max_pixels)
                && (1..=32768).contains(&images.max_dimension)
                && (1024..=268_435_456).contains(&images.max_decoded_bytes),
            "invalid image concurrency, compressed bytes, dimensions, pixels, or decoded byte limits"
        );
        ensure!(
            (100..=120000).contains(&images.work_timeout_ms)
                && images.upload_timeout_ms >= images.work_timeout_ms
                && images.upload_timeout_ms <= 180000
                && (100..=30000).contains(&images.scan_timeout_ms)
                && images.scan_timeout_ms <= images.upload_timeout_ms
                && (64..=65536).contains(&images.max_field_bytes)
                && (5..=32).contains(&images.max_fields)
                && (1024..=1_048_576).contains(&images.max_provider_response_bytes),
            "invalid image deadlines, multipart fields, or provider response limit"
        );
        let passwords = &self.passwords;
        ensure!(
            passwords.workers.get() <= 8
                && (100..=30000).contains(&passwords.work_timeout_ms)
                && (72..=4096).contains(&passwords.max_verify_bytes)
                && (bcrypt::DEFAULT_COST..=16).contains(&passwords.max_verify_cost),
            "password limits require 1..8 workers, 100..30000 milliseconds, 72..4096 verification bytes, and bcrypt cost 12..16"
        );
        let security = &self.security;
        ensure!(
            (100..=30000).contains(&security.redis_connect_timeout_ms)
                && (100..=30000).contains(&security.redis_response_timeout_ms),
            "security Redis timeouts must be 100..30000 milliseconds"
        );
        ensure!(
            (1..=64).contains(&security.max_forwarded_hops),
            "security.max_forwarded_hops must be 1..64"
        );
        for policy in [
            &security.register,
            &security.login,
            &security.forgot_password,
            &security.reset_password,
            &security.admin_login,
            &security.comment,
            &security.report,
        ] {
            ensure!(
                (1..=1_000_000).contains(&policy.requests)
                    && (1..=604800).contains(&policy.window_seconds),
                "security rate limits require 1..1000000 requests and 1..604800 seconds"
            );
        }
        ensure!(
            self.uploads_per_ip_per_day > 0,
            "upload rate limit must be positive"
        );
        ensure!(
            (100..=30000).contains(&self.email.request_timeout_ms),
            "email request timeout must be 100..30000 milliseconds"
        );
        let geo = &self.geocoding;
        let url = reqwest::Url::parse(&geo.search_url).context("parse geocoding search URL")?;
        ensure!(
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "geocoding.search_url must be HTTPS without credentials, query, or fragment"
        );
        ensure!(
            (100..=30000).contains(&geo.request_timeout_ms)
                && geo.total_timeout_ms > geo.request_timeout_ms
                && geo.total_timeout_ms <= 60000,
            "invalid geocoding timeout bounds"
        );
        ensure!(
            (1000..=60000).contains(&geo.min_interval_ms),
            "geocoding interval must be 1000..60000 milliseconds"
        );
        ensure!(
            (1..=86400).contains(&geo.failure_cooldown_seconds),
            "geocoding failure cooldown must be 1..86400 seconds"
        );
        ensure!(
            (1..=31536000).contains(&geo.positive_ttl_seconds)
                && (1..=86400).contains(&geo.negative_ttl_seconds),
            "invalid geocoding cache TTL"
        );
        ensure!(
            (1..=100000).contains(&geo.max_entries)
                && (1024..=1048576).contains(&geo.max_response_bytes),
            "invalid geocoding cache capacity"
        );
        ensure!(
            (2..=1_000_000).contains(&self.map.heat_saturation_count),
            "heat saturation must be 2..1000000 uploads"
        );
        ensure!(
            (32..=4096).contains(&self.map.max_cells),
            "map.max_cells must be 32..4096"
        );
        ensure!(
            self.map.grid_pixels.is_power_of_two() && (8..=256).contains(&self.map.grid_pixels),
            "map.grid_pixels must be a power of two from 8..256"
        );
        ensure!(
            (100..=30000).contains(&self.map.query_timeout_ms),
            "map query timeout must be 100..30000 milliseconds"
        );
        ensure!(
            (100..=2000).contains(&self.map.move_debounce_ms),
            "map debounce must be 100..2000 milliseconds"
        );
        ensure!(
            (1..=60).contains(&self.map.query_gc_seconds),
            "map query retention must be 1..60 seconds"
        );
        ensure!(
            (8..=128).contains(&self.map.heat_radius_pixels)
                && (1..=128).contains(&self.map.heat_blur_pixels),
            "invalid heatmap radius or blur"
        );
        ensure!(
            (10..=20).contains(&self.map.detail_zoom),
            "map detail zoom must be 10..20"
        );
        ensure!(
            (64..=65536).contains(&self.location_repairs.max_evidence_bytes),
            "location repair evidence must be 64..65536 bytes"
        );
        ensure!(
            (1..=60).contains(&self.web.geolocation_timeout_seconds),
            "geolocation timeout must be 1..60 seconds"
        );
        ensure!(
            self.web.dev_port > 0
                && self.web.live_dev_port > 0
                && self.web.dev_port != self.web.live_dev_port,
            "local and live ports must be nonzero and distinct"
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
            (1..=60).contains(&self.queue.poll_seconds)
                && (30..=3600).contains(&self.queue.recovery_seconds)
                && (1..=1000).contains(&self.queue.recovery_batch_size),
            "invalid job polling or recovery limits"
        );
        ensure!(
            (10..=3600).contains(&self.queue.lease_seconds)
                && self.queue.job_timeout_seconds > 0
                && self.queue.job_timeout_seconds.saturating_add(5) <= self.queue.lease_seconds,
            "job timeout must leave at least five seconds within its lease"
        );
        ensure!(
            self.queue.retry_initial_seconds > 0
                && self.queue.retry_max_seconds >= self.queue.retry_initial_seconds
                && self.queue.retry_max_seconds <= 86400,
            "queue retry bounds are invalid"
        );
        ensure!(
            (1..=100).contains(&self.queue.job_max_attempts)
                && self.queue.http_timeout_seconds > 0
                && self.queue.http_timeout_seconds <= self.queue.job_timeout_seconds,
            "invalid job attempts or HTTP timeout"
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
        ensure!(
            self.redis_budget.reserved_commands < self.redis_budget.monthly_commands,
            "Redis reserve must be smaller than the monthly budget"
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
    fn rejects_unbounded_password_work() {
        for (key, value) in [
            ("workers", 0),
            ("workers", 9),
            ("work_timeout_ms", 0),
            ("work_timeout_ms", 30001),
            ("max_verify_bytes", 71),
            ("max_verify_bytes", 4097),
            ("max_verify_cost", 11),
            ("max_verify_cost", 17),
        ] {
            let mut data = settings();
            data["passwords"][key] = value.into();
            assert!(Limits::parse(&data.to_string()).is_err(), "{key}={value}");
        }
    }

    #[test]
    fn rejects_unbounded_image_work_and_invalid_deadlines() {
        for (key, value) in [
            ("workers", 0),
            ("workers", 5),
            ("concurrent_uploads", 0),
            ("concurrent_uploads", 9),
            ("max_input_bytes", 0),
            ("max_input_bytes", 33_554_433),
            ("max_pixels", 0),
            ("max_pixels", 64_000_001),
            ("max_dimension", 0),
            ("max_dimension", 32769),
            ("max_decoded_bytes", 0),
            ("max_decoded_bytes", 268_435_457),
            ("work_timeout_ms", 0),
            ("work_timeout_ms", 120001),
            ("upload_timeout_ms", 59999),
            ("upload_timeout_ms", 180001),
            ("scan_timeout_ms", 0),
            ("scan_timeout_ms", 30001),
            ("max_field_bytes", 0),
            ("max_fields", 4),
            ("max_fields", 33),
            ("max_provider_response_bytes", 1_048_577),
        ] {
            let mut data = settings();
            data["images"][key] = value.into();
            assert!(Limits::parse(&data.to_string()).is_err(), "{key}={value}");
        }
    }

    #[test]
    fn rejects_invalid_wait_unknown_keys_and_over_budget_reserves() {
        for (section, key, value) in [
            ("queue", "poll_seconds", 0),
            ("redis_budget", "reserved_commands", 100000),
            ("cache", "fetch_lanes", 0),
        ] {
            let mut data = settings();
            data[section][key] = value.into();
            assert!(Limits::parse(&data.to_string()).is_err());
        }
        let mut data = settings();
        data["queue"]["poll_second"] = 20.into();
        assert!(Limits::parse(&data.to_string()).is_err());
    }

    #[test]
    fn rejects_unsafe_job_leases_and_attempt_budgets() {
        for (key, value) in [
            ("lease_seconds", 0),
            ("job_timeout_seconds", 300),
            ("http_timeout_seconds", 241),
            ("job_max_attempts", 0),
            ("job_max_attempts", 101),
            ("recovery_batch_size", 0),
            ("poll_seconds", 61),
        ] {
            let mut data = settings();
            data["queue"][key] = value.into();
            assert!(Limits::parse(&data.to_string()).is_err(), "{key}={value}");
        }
    }

    #[test]
    fn rejects_invalid_geolocation_timeouts() {
        for value in [0, 61] {
            let mut data = settings();
            data["web"]["geolocation_timeout_seconds"] = value.into();
            assert!(Limits::parse(&data.to_string()).is_err());
        }
    }

    #[test]
    fn rejects_invalid_email_timeouts() {
        for value in [0, 99, 30001] {
            let mut data = settings();
            data["email"]["request_timeout_ms"] = value.into();
            assert!(Limits::parse(&data.to_string()).is_err());
        }
    }

    #[test]
    fn rejects_disabled_or_unbounded_abuse_controls() {
        for (key, value) in [
            ("redis_connect_timeout_ms", 0),
            ("redis_response_timeout_ms", 30001),
            ("max_forwarded_hops", 0),
            ("max_forwarded_hops", 65),
        ] {
            let mut data = settings();
            data["security"][key] = value.into();
            assert!(Limits::parse(&data.to_string()).is_err());
        }
        for section in [
            "register",
            "login",
            "forgot_password",
            "reset_password",
            "admin_login",
            "comment",
            "report",
        ] {
            for (key, value) in [
                ("requests", 0),
                ("requests", 1000001),
                ("window_seconds", 0),
                ("window_seconds", 604801),
            ] {
                let mut data = settings();
                data["security"][section][key] = value.into();
                assert!(Limits::parse(&data.to_string()).is_err());
            }
        }
        let mut data = settings();
        data["uploads_per_ip_per_day"] = 0.into();
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
        data["queue"]["lease_seconds"] = u64::MAX.into();
        assert!(Limits::parse(&data.to_string()).is_err());
    }
    #[test]
    fn rejects_unbounded_location_evidence() {
        for value in [0, 65537] {
            let mut data = settings();
            data["location_repairs"]["max_evidence_bytes"] = value.into();
            assert!(Limits::parse(&data.to_string()).is_err());
        }
    }
    #[test]
    fn rejects_unbounded_map_work() {
        for (key, value) in [
            ("max_cells", 0),
            ("max_cells", 4097),
            ("grid_pixels", 31),
            ("query_timeout_ms", 0),
            ("move_debounce_ms", 0),
            ("query_gc_seconds", 61),
            ("heat_radius_pixels", 0),
            ("heat_blur_pixels", 0),
            ("detail_zoom", 23),
        ] {
            let mut data = settings();
            data["map"][key] = value.into();
            assert!(Limits::parse(&data.to_string()).is_err(), "{key}");
        }
    }
    #[test]
    fn rejects_invalid_geocoding_limits() {
        for (key, value) in [
            ("request_timeout_ms", 0),
            ("total_timeout_ms", 6000),
            ("min_interval_ms", 999),
            ("min_interval_ms", 60001),
            ("failure_cooldown_seconds", 0),
            ("positive_ttl_seconds", 0),
            ("negative_ttl_seconds", 86401),
            ("max_entries", 0),
            ("max_response_bytes", 1048577),
        ] {
            let mut data = settings();
            data["geocoding"][key] = value.into();
            assert!(Limits::parse(&data.to_string()).is_err(), "{key}");
        }
        for url in [
            "http://example.com/search",
            "https://user:pass@example.com/search",
            "https://example.com/search?q=x",
            "not a URL",
        ] {
            let mut data = settings();
            data["geocoding"]["search_url"] = url.into();
            assert!(Limits::parse(&data.to_string()).is_err(), "{url}");
        }
    }
}
