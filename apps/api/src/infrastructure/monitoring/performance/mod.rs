//! Performance monitoring service for production observability.
//!
//! Comprehensive performance monitoring for tracking application performance, business metrics,
//! and operational health indicators across all service components.

pub mod config;
pub mod types;

pub use config::MonitorConfig;
pub use types::*;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn, instrument};
use uuid::Uuid;

/// Internal monitoring state with categorized collectors
#[derive(Default)]
pub struct MonitorInner {
    /// HTTP request metrics indexed by endpoint
    pub http_metrics: HashMap<String, HttpMetrics>,
    /// Database query performance metrics
    pub db_metrics: HashMap<String, DatabaseMetrics>,
    /// Business domain metrics
    pub business_metrics: BusinessMetrics,
    /// System resource utilization
    pub resource_metrics: ResourceMetrics,
    /// Custom application metrics
    pub custom_metrics: HashMap<String, CustomMetric>,
    /// Error tracking by category
    pub error_metrics: HashMap<String, ErrorMetrics>,
}

/// Comprehensive performance monitoring service for production observability.
///
/// Tracks application performance, business metrics, and operational health indicators
/// across all service components. Provides both real-time monitoring capabilities and
/// historical trend analysis for capacity planning and incident response.
pub struct PerformanceMonitor {
    /// Thread-safe storage for metric data points
    pub inner: Arc<RwLock<MonitorInner>>,

    /// Service start time for uptime calculation
    start_time: Instant,

    /// Configuration for monitoring behavior
    config: MonitorConfig,
}

impl PerformanceMonitor {
    /// Creates a new performance monitor with default configuration
    pub fn new() -> Self {
        Self::with_config(MonitorConfig::default())
    }

    /// Creates a performance monitor with custom configuration
    pub fn with_config(config: MonitorConfig) -> Self {
        info!("Initializing PerformanceMonitor with configuration: {:?}", config);

        Self {
            inner: Arc::new(RwLock::new(MonitorInner::default())),
            start_time: Instant::now(),
            config,
        }
    }

    /// Records an HTTP request completion with comprehensive metrics
    #[instrument(skip(self), fields(endpoint = %endpoint, status = status_code, duration_ms = duration.as_millis()))]
    pub async fn record_http_request(
        &self,
        endpoint: &str,
        method: &str,
        status_code: u16,
        duration: Duration,
        concurrent_requests: u32,
    ) {
        let mut inner = self.inner.write().await;
        let key = format!("{}:{}", method, endpoint);

        let should_record_error = matches!(status_code, 400..=599);
        let is_critical_error = matches!(status_code, 500..=599);

        let metrics = inner.http_metrics.entry(key.clone()).or_default();

        metrics.total_requests += 1;
        metrics.concurrent_requests = concurrent_requests;
        metrics.response_times.push(duration.as_millis() as u64);

        match status_code {
            200..=299 => metrics.successful_requests += 1,
            400..=499 => {
                metrics.client_errors += 1;
                debug!("Client error {} on endpoint {}", status_code, endpoint);
            }
            500..=599 => {
                metrics.server_errors += 1;
                warn!("Server error {} on endpoint {}: took {:?}", status_code, endpoint, duration);
            }
            _ => {}
        }

        let now = Instant::now();
        if let Some(last_time) = metrics.last_request_time {
            let time_diff = now.duration_since(last_time).as_secs() as f64 / 60.0;
            if time_diff > 0.0 {
                metrics.requests_per_minute = 1.0 / time_diff;
            }
        }
        metrics.last_request_time = Some(now);

        if metrics.response_times.len() > self.config.max_data_points {
            metrics.response_times.drain(0..100);
        }

        if should_record_error {
            let error_metrics = inner.error_metrics.entry(key.clone()).or_default();
            error_metrics.record_error(status_code, is_critical_error);
        }

        if duration.as_millis() as u64 > self.config.high_response_time_threshold_ms {
            self.create_alert(
                AlertSeverity::Warning,
                "High Response Time",
                &format!("Endpoint {} took {}ms", endpoint, duration.as_millis()),
                "response_time",
                self.config.high_response_time_threshold_ms as f64,
                duration.as_millis() as f64,
            ).await;
        }
    }

    /// Records database query execution metrics with detailed analysis
    #[instrument(skip(self), fields(query_type = %query_type, duration_ms = duration.as_millis()))]
    pub async fn record_database_query(
        &self,
        query_type: &str,
        duration: Duration,
        rows_affected: u64,
        success: bool,
        pool_utilization: f32,
    ) {
        let mut inner = self.inner.write().await;
        let metrics = inner.db_metrics.entry(query_type.to_string()).or_default();

        metrics.total_queries += 1;
        let duration_ms = duration.as_millis() as u64;
        metrics.execution_times.push(duration_ms);
        metrics.connection_pool_usage.push(pool_utilization);

        if success {
            metrics.successful_queries += 1;

            let total_successful = metrics.successful_queries as f64;
            metrics.average_rows_affected =
                (metrics.average_rows_affected * (total_successful - 1.0) + rows_affected as f64) / total_successful;
        } else {
            metrics.failed_queries += 1;
            warn!("Database query failed: type={}, duration={:?}", query_type, duration);
        }

        if duration_ms > self.config.slow_query_threshold_ms {
            metrics.slow_queries += 1;
            warn!("Slow query detected: type={}, duration={}ms", query_type, duration_ms);
        }

        if metrics.execution_times.len() > self.config.max_data_points {
            metrics.execution_times.drain(0..100);
        }
    }

    /// Records business metric events for product analytics
    #[instrument(skip(self))]
    pub async fn record_business_event(&self, event: BusinessEvent) {
        let mut inner = self.inner.write().await;
        let business = &mut inner.business_metrics;

        match event {
            BusinessEvent::UserActivity { user_id: _ } => {
                business.daily_active_users += 1;
            }
            BusinessEvent::LetteringUploaded { country_code } => {
                business.total_uploads += 1;
                *business.uploads_by_country.entry(country_code).or_insert(0) += 1;
            }
            BusinessEvent::LetteringApproved => {
                let approved_count = business.total_uploads as f64 * business.upload_approval_rate + 1.0;
                if business.total_uploads > 0 {
                    business.upload_approval_rate = approved_count / business.total_uploads as f64;
                }
            }
            BusinessEvent::LetteringRejected { reason: _ } => {}
            BusinessEvent::UserEngagement { engagement_type } => {
                match engagement_type {
                    EngagementType::Like => business.total_likes += 1,
                    EngagementType::Comment => business.total_comments += 1,
                    EngagementType::Report => business.total_reports += 1,
                    EngagementType::Share | EngagementType::Download => {}
                }
            }
            BusinessEvent::ModerationCompleted { duration_hours } => {
                let total_moderated = (business.average_moderation_time_hours * 100.0) as u64 + 1;
                business.average_moderation_time_hours =
                    (business.average_moderation_time_hours * (total_moderated - 1) as f64 + duration_hours) / total_moderated as f64;
            }
            BusinessEvent::DuplicateDetected => {
                if business.total_uploads > 0 {
                    business.duplicate_detection_rate =
                        (business.duplicate_detection_rate * business.total_uploads as f64 + 1.0) / (business.total_uploads + 1) as f64;
                }
            }
            BusinessEvent::CacheHit { cache_type: _ } => {
                let total_cache_ops = business.cache_hit_rate + business.cache_miss_rate;
                if total_cache_ops > 0.0 {
                    business.cache_hit_rate = (business.cache_hit_rate * total_cache_ops + 1.0) / (total_cache_ops + 1.0);
                } else {
                    business.cache_hit_rate = 1.0;
                }
            }
            BusinessEvent::CacheMiss { cache_type: _ } => {
                let total_cache_ops = business.cache_hit_rate + business.cache_miss_rate;
                if total_cache_ops > 0.0 {
                    business.cache_miss_rate = (business.cache_miss_rate * total_cache_ops + 1.0) / (total_cache_ops + 1.0);
                } else {
                    business.cache_miss_rate = 1.0;
                }
            }
            BusinessEvent::MlProcessingCompleted { success, processing_time_ms: _ } => {
                let current_rate = business.ml_processing_success_rate;
                let total_processed = if current_rate > 0.0 { 100.0 / current_rate } else { 1.0 };

                if success {
                    business.ml_processing_success_rate =
                        (current_rate * total_processed + 1.0) / (total_processed + 1.0);
                } else {
                    business.ml_processing_success_rate =
                        (current_rate * total_processed) / (total_processed + 1.0);
                }
            }
        }
    }

    /// Updates system resource utilization metrics
    #[instrument(skip(self))]
    pub async fn update_resource_metrics(
        &self,
        memory_mb: f64,
        cpu_percent: f64,
        db_pool_active: u32,
        db_pool_idle: u32,
        db_pool_max: u32,
        redis_memory_mb: f64,
        redis_clients: u32,
    ) {
        let mut inner = self.inner.write().await;
        let resources = &mut inner.resource_metrics;

        resources.memory_usage_mb = memory_mb;
        resources.cpu_usage_percent = cpu_percent;
        resources.db_pool_active_connections = db_pool_active;
        resources.db_pool_idle_connections = db_pool_idle;
        resources.db_pool_max_connections = db_pool_max;
        resources.redis_memory_usage_mb = redis_memory_mb;
        resources.redis_connected_clients = redis_clients;

        if cpu_percent > self.config.cpu_usage_threshold_percent {
            self.create_alert(
                AlertSeverity::Warning,
                "High CPU Usage",
                &format!("CPU usage at {:.1}%", cpu_percent),
                "cpu_usage",
                self.config.cpu_usage_threshold_percent,
                cpu_percent,
            ).await;
        }

        let memory_percent = (memory_mb / 1024.0) * 100.0;
        if memory_percent > self.config.memory_usage_threshold_percent {
            self.create_alert(
                AlertSeverity::Warning,
                "High Memory Usage",
                &format!("Memory usage at {:.1}%", memory_percent),
                "memory_usage",
                self.config.memory_usage_threshold_percent,
                memory_percent,
            ).await;
        }
    }

    /// Creates an alert for monitoring systems
    async fn create_alert(
        &self,
        severity: AlertSeverity,
        title: &str,
        description: &str,
        metric: &str,
        threshold: f64,
        current_value: f64,
    ) {
        let alert = Alert {
            id: Uuid::now_v7().to_string(),
            severity,
            title: title.to_string(),
            description: description.to_string(),
            metric: metric.to_string(),
            threshold,
            current_value,
            created_at: chrono::Utc::now(),
            resolved_at: None,
        };

        warn!("Alert created: {:?}", alert);
    }

    /// Generates comprehensive performance report for monitoring dashboards
    pub async fn generate_snapshot(&self) -> MetricsSnapshot {
        let inner = self.inner.read().await;
        let uptime = self.start_time.elapsed().as_secs();

        let http_summary = self.calculate_http_summary(&inner.http_metrics);
        let database_summary = self.calculate_database_summary(&inner.db_metrics);
        let business_summary = self.calculate_business_summary(&inner.business_metrics);
        let resource_summary = self.calculate_resource_summary(&inner.resource_metrics);
        let error_summary = self.calculate_error_summary(&inner.error_metrics);
        let health_indicators = self.calculate_health_indicators(&inner);

        MetricsSnapshot {
            timestamp: chrono::Utc::now(),
            uptime_seconds: uptime,
            http_summary,
            database_summary,
            business_summary,
            resource_summary,
            error_summary,
            health_indicators,
            active_alerts: vec![],
        }
    }

    // ===== Internal Calculation Methods =====

    fn calculate_http_summary(&self, metrics: &HashMap<String, HttpMetrics>) -> HttpSummary {
        let mut total_requests = 0;
        let mut successful_requests = 0;
        let mut all_response_times = Vec::new();
        let mut requests_per_minute = 0.0;
        let mut concurrent_requests = 0;

        for metrics in metrics.values() {
            total_requests += metrics.total_requests;
            successful_requests += metrics.successful_requests;
            all_response_times.extend(&metrics.response_times);
            requests_per_minute += metrics.requests_per_minute;
            concurrent_requests = concurrent_requests.max(metrics.concurrent_requests);
        }

        all_response_times.sort_unstable();
        let avg = if !all_response_times.is_empty() {
            all_response_times.iter().sum::<u64>() as f64 / all_response_times.len() as f64
        } else {
            0.0
        };
        let p50 = Self::calculate_percentile(&all_response_times, 50.0);
        let p95 = Self::calculate_percentile(&all_response_times, 95.0);
        let p99 = Self::calculate_percentile(&all_response_times, 99.0);

        let success_rate = if total_requests > 0 {
            successful_requests as f64 / total_requests as f64
        } else {
            0.0
        };

        HttpSummary {
            total_requests,
            requests_per_minute: if !metrics.is_empty() { requests_per_minute / metrics.len() as f64 } else { 0.0 },
            success_rate,
            error_rate: 1.0 - success_rate,
            avg_response_time_ms: avg,
            p50_response_time_ms: p50,
            p95_response_time_ms: p95,
            p99_response_time_ms: p99,
            concurrent_requests,
            slowest_endpoints: vec![],
        }
    }

    fn calculate_database_summary(&self, metrics: &HashMap<String, DatabaseMetrics>) -> DatabaseSummary {
        let mut total_queries = 0;
        let mut successful_queries = 0;
        let mut slow_query_count = 0;
        let mut all_execution_times = Vec::new();
        let mut pool_utilization_sum = 0.0;
        let mut pool_measurements = 0;

        for metrics in metrics.values() {
            total_queries += metrics.total_queries;
            successful_queries += metrics.successful_queries;
            slow_query_count += metrics.slow_queries;
            all_execution_times.extend(&metrics.execution_times);

            pool_utilization_sum += metrics.connection_pool_usage.iter().sum::<f32>() as f64;
            pool_measurements += metrics.connection_pool_usage.len();
        }

        all_execution_times.sort_unstable();
        let avg_execution_time = if !all_execution_times.is_empty() {
            all_execution_times.iter().sum::<u64>() as f64 / all_execution_times.len() as f64
        } else {
            0.0
        };

        DatabaseSummary {
            total_queries,
            queries_per_second: 0.0,
            success_rate: if total_queries > 0 { successful_queries as f64 / total_queries as f64 } else { 0.0 },
            avg_execution_time_ms: avg_execution_time,
            p95_execution_time_ms: Self::calculate_percentile(&all_execution_times, 95.0),
            slow_query_count,
            connection_pool_utilization: if pool_measurements > 0 { pool_utilization_sum / pool_measurements as f64 } else { 0.0 },
            deadlock_count: 0,
        }
    }

    fn calculate_business_summary(&self, business: &BusinessMetrics) -> BusinessSummary {
        let engagement_rate = if business.total_uploads > 0 {
            (business.total_likes + business.total_comments) as f64 / business.total_uploads as f64
        } else {
            0.0
        };

        let content_quality_score = (business.upload_approval_rate + business.duplicate_detection_rate + business.ml_processing_success_rate) / 3.0;

        BusinessSummary {
            daily_active_users: business.daily_active_users,
            upload_volume_24h: business.total_uploads,
            approval_rate: business.upload_approval_rate,
            engagement_rate,
            cache_hit_rate: business.cache_hit_rate,
            ml_processing_success_rate: business.ml_processing_success_rate,
            moderation_backlog: business.pending_moderation_queue_size,
            content_quality_score,
        }
    }

    fn calculate_resource_summary(&self, resources: &ResourceMetrics) -> ResourceSummary {
        let db_connection_usage = if resources.db_pool_max_connections > 0 {
            resources.db_pool_active_connections as f64 / resources.db_pool_max_connections as f64
        } else {
            0.0
        };

        ResourceSummary {
            memory_usage_percent: resources.memory_usage_mb / 10.24,
            cpu_usage_percent: resources.cpu_usage_percent,
            database_connection_usage: db_connection_usage,
            redis_memory_usage_mb: resources.redis_memory_usage_mb,
            storage_performance_score: resources.storage_upload_success_rate,
            network_utilization: resources.network_throughput_mbps(),
            disk_utilization: (resources.disk_reads_per_sec + resources.disk_writes_per_sec) / 100.0,
        }
    }

    fn calculate_error_summary(&self, errors: &HashMap<String, ErrorMetrics>) -> ErrorSummary {
        let mut total_errors = 0;
        let mut critical_errors = 0;
        let mut error_rates = Vec::new();
        let mut recovery_times = Vec::new();
        let mut all_errors_by_status: HashMap<u16, u64> = HashMap::new();

        for metrics in errors.values() {
            total_errors += metrics.total_errors;
            critical_errors += metrics.critical_errors;
            error_rates.push(metrics.error_rate);
            if metrics.average_recovery_time != Duration::ZERO {
                recovery_times.push(metrics.average_recovery_time.as_secs_f64() / 60.0);
            }

            for (status_code, count) in metrics.error_breakdown() {
                *all_errors_by_status.entry(status_code).or_insert(0) += count;
            }
        }

        let avg_error_rate = if !error_rates.is_empty() {
            error_rates.iter().sum::<f64>() / error_rates.len() as f64
        } else {
            0.0
        };

        let avg_recovery_time = if !recovery_times.is_empty() {
            recovery_times.iter().sum::<f64>() / recovery_times.len() as f64
        } else {
            0.0
        };

        let mut error_types: Vec<_> = all_errors_by_status.iter().collect();
        error_types.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        let top_error_types: Vec<(String, u64)> = error_types.iter()
            .take(5)
            .map(|(status, count)| (format!("HTTP {}", status), **count))
            .collect();

        ErrorSummary {
            total_errors_24h: total_errors,
            error_rate: avg_error_rate,
            critical_errors,
            top_error_types,
            average_recovery_time_minutes: avg_recovery_time,
            errors_by_hour: vec![],
        }
    }

    fn calculate_health_indicators(&self, inner: &MonitorInner) -> HealthIndicators {
        let overall_health = self.assess_overall_health(inner);

        HealthIndicators {
            overall_health: overall_health.clone(),
            api_health: self.assess_api_health(&inner.http_metrics),
            database_health: self.assess_database_health(&inner.db_metrics),
            cache_health: HealthStatus::Healthy,
            storage_health: HealthStatus::Healthy,
            ml_service_health: HealthStatus::Healthy,
            last_health_check: chrono::Utc::now(),
        }
    }

    fn assess_overall_health(&self, inner: &MonitorInner) -> HealthStatus {
        let api_health = self.assess_api_health(&inner.http_metrics);
        let db_health = self.assess_database_health(&inner.db_metrics);
        let resource_health = self.assess_resource_health(&inner.resource_metrics);

        match (api_health, db_health, resource_health) {
            (HealthStatus::Critical, _, _) | (_, HealthStatus::Critical, _) | (_, _, HealthStatus::Critical) => HealthStatus::Critical,
            (HealthStatus::Unhealthy, _, _) | (_, HealthStatus::Unhealthy, _) | (_, _, HealthStatus::Unhealthy) => HealthStatus::Unhealthy,
            (HealthStatus::Degraded, _, _) | (_, HealthStatus::Degraded, _) | (_, _, HealthStatus::Degraded) => HealthStatus::Degraded,
            _ => HealthStatus::Healthy,
        }
    }

    fn assess_api_health(&self, http_metrics: &HashMap<String, HttpMetrics>) -> HealthStatus {
        if http_metrics.is_empty() {
            return HealthStatus::Healthy;
        }

        let mut total_requests = 0;
        let mut total_errors = 0;
        let mut avg_response_times = Vec::new();

        for metrics in http_metrics.values() {
            total_requests += metrics.total_requests;
            total_errors += metrics.client_errors + metrics.server_errors;

            if !metrics.response_times.is_empty() {
                let avg = metrics.response_times.iter().sum::<u64>() as f64 / metrics.response_times.len() as f64;
                avg_response_times.push(avg);
            }
        }

        let error_rate = if total_requests > 0 {
            total_errors as f64 / total_requests as f64
        } else {
            0.0
        };

        let avg_response_time = if !avg_response_times.is_empty() {
            avg_response_times.iter().sum::<f64>() / avg_response_times.len() as f64
        } else {
            0.0
        };

        if error_rate > 0.1 || avg_response_time > 10000.0 {
            HealthStatus::Critical
        } else if error_rate > 0.05 || avg_response_time > 5000.0 {
            HealthStatus::Unhealthy
        } else if error_rate > 0.02 || avg_response_time > 2000.0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }

    fn assess_database_health(&self, db_metrics: &HashMap<String, DatabaseMetrics>) -> HealthStatus {
        if db_metrics.is_empty() {
            return HealthStatus::Healthy;
        }

        let mut total_queries = 0;
        let mut failed_queries = 0;
        let mut slow_queries = 0;
        let mut avg_execution_times = Vec::new();

        for metrics in db_metrics.values() {
            total_queries += metrics.total_queries;
            failed_queries += metrics.failed_queries;
            slow_queries += metrics.slow_queries;

            if !metrics.execution_times.is_empty() {
                let avg = metrics.execution_times.iter().sum::<u64>() as f64 / metrics.execution_times.len() as f64;
                avg_execution_times.push(avg);
            }
        }

        let failure_rate = if total_queries > 0 {
            failed_queries as f64 / total_queries as f64
        } else {
            0.0
        };

        let slow_query_rate = if total_queries > 0 {
            slow_queries as f64 / total_queries as f64
        } else {
            0.0
        };

        let avg_execution_time = if !avg_execution_times.is_empty() {
            avg_execution_times.iter().sum::<f64>() / avg_execution_times.len() as f64
        } else {
            0.0
        };

        if failure_rate > 0.1 || slow_query_rate > 0.5 || avg_execution_time > 5000.0 {
            HealthStatus::Critical
        } else if failure_rate > 0.05 || slow_query_rate > 0.2 || avg_execution_time > 2000.0 {
            HealthStatus::Unhealthy
        } else if failure_rate > 0.02 || slow_query_rate > 0.1 || avg_execution_time > 1000.0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }

    fn assess_resource_health(&self, resource_metrics: &ResourceMetrics) -> HealthStatus {
        let memory_percent = (resource_metrics.memory_usage_mb / 1024.0) * 100.0;
        let cpu_percent = resource_metrics.cpu_usage_percent;

        let db_pool_utilization = if resource_metrics.db_pool_max_connections > 0 {
            resource_metrics.db_pool_active_connections as f64 / resource_metrics.db_pool_max_connections as f64 * 100.0
        } else {
            0.0
        };

        if memory_percent > 95.0 || cpu_percent > 95.0 || db_pool_utilization > 95.0 {
            HealthStatus::Critical
        } else if memory_percent > 85.0 || cpu_percent > 85.0 || db_pool_utilization > 85.0 {
            HealthStatus::Unhealthy
        } else if memory_percent > 70.0 || cpu_percent > 70.0 || db_pool_utilization > 70.0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }

    fn calculate_percentile(sorted_data: &[u64], percentile: f64) -> f64 {
        if sorted_data.is_empty() {
            return 0.0;
        }

        let len = sorted_data.len() as f64;
        let rank = (percentile / 100.0 * len).ceil() as usize;
        
        let index = if rank > 0 { rank - 1 } else { 0 };
        sorted_data[index.min(sorted_data.len() - 1)] as f64
    }

    pub async fn record_storage_operation(&self, success: bool, duration_ms: f64, bytes_transferred: u64) {
        let mut inner = self.inner.write().await;
        inner.resource_metrics.record_storage_upload(success, duration_ms);
        if success {
            inner.resource_metrics.record_network_io(bytes_transferred, 0);
        }
    }

    pub async fn update_disk_io_metrics(&self, reads_per_sec: f64, writes_per_sec: f64) {
        let mut inner = self.inner.write().await;
        inner.resource_metrics.update_disk_io(reads_per_sec, writes_per_sec);
    }

    pub async fn get_error_breakdown(&self) -> HashMap<String, Vec<(u16, u64)>> {
        let inner = self.inner.read().await;
        inner.error_metrics.iter()
            .map(|(k, v)| (k.clone(), v.error_breakdown()))
            .collect()
    }

    pub async fn update_error_rates(&self, time_window_secs: u64) {
        let mut inner = self.inner.write().await;
        for error_metrics in inner.error_metrics.values_mut() {
            error_metrics.update_error_rate(time_window_secs);
        }
    }

    pub async fn get_time_since_last_error(&self, endpoint: &str) -> Option<u64> {
        let inner = self.inner.read().await;
        inner.error_metrics.get(endpoint)
            .and_then(|metrics| metrics.time_since_last_error())
    }

    pub async fn cleanup_old_metrics(&self) {
        if !self.config.enable_automatic_cleanup {
            return;
        }

        let mut inner = self.inner.write().await;
        let retention_threshold = Instant::now() - Duration::from_secs(self.config.cleanup_interval_minutes * 60);

        for metrics in inner.http_metrics.values_mut() {
            if metrics.response_times.len() > self.config.max_data_points {
                metrics.response_times.drain(0..(metrics.response_times.len() - self.config.max_data_points));
            }
        }

        for metrics in inner.db_metrics.values_mut() {
            if metrics.execution_times.len() > self.config.max_data_points {
                metrics.execution_times.drain(0..(metrics.execution_times.len() - self.config.max_data_points));
            }
            if metrics.connection_pool_usage.len() > self.config.max_data_points {
                metrics.connection_pool_usage.drain(0..(metrics.connection_pool_usage.len() - self.config.max_data_points));
            }
        }

        for metric in inner.custom_metrics.values_mut() {
            metric.data_points.retain(|(timestamp, _)| *timestamp > retention_threshold);
        }

        debug!("Completed automatic cleanup of old metrics");
    }

    pub async fn register_custom_metric(
        &self,
        name: String,
        description: String,
        metric_type: MetricType,
        labels: HashMap<String, String>,
        warning_threshold: Option<f64>,
        critical_threshold: Option<f64>,
    ) {
        let mut inner = self.inner.write().await;
        let metric = CustomMetric::new(
            name.clone(),
            description,
            metric_type,
            labels,
            warning_threshold,
            critical_threshold,
        );
        inner.custom_metrics.insert(name, metric);
    }

    pub async fn record_custom_metric(&self, name: &str, value: f64) {
        let mut inner = self.inner.write().await;
        if let Some(metric) = inner.custom_metrics.get_mut(name) {
            metric.record(value);

            if let Some(critical) = metric.critical_threshold() {
                if value > critical {
                    self.create_alert(
                        AlertSeverity::Critical,
                        &format!("Critical threshold exceeded for {}", name),
                        &format!("Value {} exceeds critical threshold {}", value, critical),
                        name,
                        critical,
                        value,
                    ).await;
                }
            } else if let Some(warning) = metric.warning_threshold() {
                if value > warning {
                    self.create_alert(
                        AlertSeverity::Warning,
                        &format!("Warning threshold exceeded for {}", name),
                        &format!("Value {} exceeds warning threshold {}", value, warning),
                        name,
                        warning,
                        value,
                    ).await;
                }
            }
        }
    }

    pub async fn get_custom_metrics_summary(&self) -> Vec<CustomMetricSummary> {
        let inner = self.inner.read().await;
        inner.custom_metrics.values().map(|metric| {
            CustomMetricSummary {
                name: metric.name().to_string(),
                description: metric.description().to_string(),
                current_value: metric.current_value(),
                labels: metric.labels().clone(),
                warning_threshold: metric.warning_threshold(),
                critical_threshold: metric.critical_threshold(),
            }
        }).collect()
    }
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_http_request_recording() {
        let monitor = PerformanceMonitor::new();

        monitor.record_http_request("/api/v1/letterings", "GET", 200, Duration::from_millis(150), 5).await;
        monitor.record_http_request("/api/v1/letterings", "GET", 404, Duration::from_millis(50), 3).await;

        let snapshot = monitor.generate_snapshot().await;
        assert_eq!(snapshot.http_summary.total_requests, 2);
        assert_eq!(snapshot.http_summary.success_rate, 0.5);
    }

    #[tokio::test]
    async fn test_database_query_recording() {
        let monitor = PerformanceMonitor::new();

        monitor.record_database_query("SELECT", Duration::from_millis(100), 5, true, 0.5).await;
        monitor.record_database_query("INSERT", Duration::from_millis(200), 1, false, 0.7).await;

        let snapshot = monitor.generate_snapshot().await;
        assert_eq!(snapshot.database_summary.total_queries, 2);
        assert_eq!(snapshot.database_summary.success_rate, 0.5);
    }

    #[tokio::test]
    async fn test_business_event_recording() {
        let monitor = PerformanceMonitor::new();

        monitor.record_business_event(BusinessEvent::LetteringUploaded {
            country_code: "IN".to_string()
        }).await;
        monitor.record_business_event(BusinessEvent::LetteringApproved).await;
        monitor.record_business_event(BusinessEvent::UserEngagement {
            engagement_type: EngagementType::Like
        }).await;

        let snapshot = monitor.generate_snapshot().await;
        assert_eq!(snapshot.business_summary.upload_volume_24h, 1);
        assert!(snapshot.business_summary.approval_rate > 0.0);
    }

    #[tokio::test]
    async fn test_custom_metric_registration() {
        let monitor = PerformanceMonitor::new();
        let mut labels = HashMap::new();
        labels.insert("service".to_string(), "upload".to_string());

        monitor.register_custom_metric(
            "upload_queue_size".to_string(),
            "Number of items in upload processing queue".to_string(),
            MetricType::Gauge,
            labels,
            Some(50.0),
            Some(100.0),
        ).await;

        monitor.record_custom_metric("upload_queue_size", 25.0).await;
        monitor.record_custom_metric("upload_queue_size", 75.0).await;

        let inner = monitor.inner.read().await;
        let custom_metrics = &inner.custom_metrics;
        assert!(custom_metrics.contains_key("upload_queue_size"));

        let metric = &custom_metrics["upload_queue_size"];
        assert_eq!(metric.data_points.len(), 2);
        assert_eq!(metric.data_points[1].1, 75.0);
    }

    #[tokio::test]
    async fn test_health_assessment() {
        let monitor = PerformanceMonitor::new();

        monitor.record_http_request("/api/test", "GET", 200, Duration::from_millis(100), 1).await;
        monitor.record_database_query("SELECT", Duration::from_millis(50), 10, true, 0.3).await;
        monitor.update_resource_metrics(512.0, 25.0, 5, 15, 20, 128.0, 10).await;

        let snapshot = monitor.generate_snapshot().await;
        assert_eq!(snapshot.health_indicators.overall_health, HealthStatus::Healthy);
        assert_eq!(snapshot.health_indicators.api_health, HealthStatus::Healthy);
        assert_eq!(snapshot.health_indicators.database_health, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_percentile_calculation() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        assert_eq!(PerformanceMonitor::calculate_percentile(&data, 50.0), 5.0);
        assert_eq!(PerformanceMonitor::calculate_percentile(&data, 95.0), 10.0);
        assert_eq!(PerformanceMonitor::calculate_percentile(&[], 50.0), 0.0);
    }
}
