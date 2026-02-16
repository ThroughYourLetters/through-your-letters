//! Metric types and data structures for performance monitoring.
//!
//! This module defines all the core metric types used throughout the monitoring system,
//! including specialized structures for HTTP, database, and business metrics,
//! as well as summary types for dashboards and alerting.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

// ===== Metric Collection Types =====

/// HTTP request performance and error tracking per endpoint
#[derive(Default, Clone)]
pub struct HttpMetrics {
    /// Total number of requests processed
    pub total_requests: u64,
    /// Requests completed successfully (2xx status)
    pub successful_requests: u64,
    /// Client error requests (4xx status)
    pub client_errors: u64,
    /// Server error requests (5xx status)
    pub server_errors: u64,
    /// Response time measurements in milliseconds
    pub response_times: Vec<u64>,
    /// Request rate per minute (sliding window)
    pub requests_per_minute: f64,
    /// Last request timestamp for rate calculation
    pub last_request_time: Option<Instant>,
    /// Concurrent request count
    pub concurrent_requests: u32,
}

/// Database query performance tracking by operation type
#[derive(Default, Clone)]
pub struct DatabaseMetrics {
    /// Total query execution count
    pub total_queries: u64,
    /// Successful query completions
    pub successful_queries: u64,
    /// Failed queries (timeouts, errors, deadlocks)
    pub failed_queries: u64,
    /// Query execution times in milliseconds
    pub execution_times: Vec<u64>,
    /// Average rows affected/returned
    pub average_rows_affected: f64,
    /// Connection pool utilization when query was executed
    pub connection_pool_usage: Vec<f32>,
    /// Slow query count (above threshold)
    pub slow_queries: u64,
}

/// Business-specific metrics for product analytics and monitoring
#[derive(Default, Clone)]
pub struct BusinessMetrics {
    /// Daily active users (unique IPs/authenticated users)
    pub daily_active_users: u64,
    /// Total lettering uploads processed
    pub total_uploads: u64,
    /// Upload approval rate (approved/total)
    pub upload_approval_rate: f64,
    /// Community engagement metrics
    pub total_likes: u64,
    pub total_comments: u64,
    pub total_reports: u64,
    /// Geographic distribution of content
    pub uploads_by_country: HashMap<String, u64>,
    /// Content quality indicators
    pub duplicate_detection_rate: f64,
    pub ml_processing_success_rate: f64,
    /// Moderation efficiency metrics
    pub average_moderation_time_hours: f64,
    pub pending_moderation_queue_size: u64,
    /// Cache performance
    pub cache_hit_rate: f64,
    pub cache_miss_rate: f64,
}

/// System resource utilization tracking
#[derive(Default, Clone)]
pub struct ResourceMetrics {
    /// Memory usage in MB
    pub memory_usage_mb: f64,
    /// CPU utilization percentage
    pub cpu_usage_percent: f64,
    /// Database connection pool statistics
    pub db_pool_active_connections: u32,
    pub db_pool_idle_connections: u32,
    pub db_pool_max_connections: u32,
    /// Redis connection and memory usage
    pub redis_memory_usage_mb: f64,
    pub redis_connected_clients: u32,
    /// Storage service metrics
    pub storage_upload_success_rate: f64,
    pub storage_avg_upload_time_ms: f64,
    /// Network I/O metrics
    pub network_bytes_sent: u64,
    pub network_bytes_received: u64,
    /// Disk I/O metrics
    pub disk_reads_per_sec: f64,
    pub disk_writes_per_sec: f64,
}

impl ResourceMetrics {
    /// Records storage upload metrics
    pub fn record_storage_upload(&mut self, success: bool, duration_ms: f64) {
        let weight = 0.1;
        let success_value = if success { 1.0 } else { 0.0 };
        self.storage_upload_success_rate =
            self.storage_upload_success_rate * (1.0 - weight) + success_value * weight;

        if success {
            self.storage_avg_upload_time_ms =
                self.storage_avg_upload_time_ms * (1.0 - weight) + duration_ms * weight;
        }
    }

    /// Records network I/O activity
    pub fn record_network_io(&mut self, bytes_sent: u64, bytes_received: u64) {
        self.network_bytes_sent += bytes_sent;
        self.network_bytes_received += bytes_received;
    }

    /// Updates disk I/O rates
    pub fn update_disk_io(&mut self, reads_per_sec: f64, writes_per_sec: f64) {
        self.disk_reads_per_sec = reads_per_sec;
        self.disk_writes_per_sec = writes_per_sec;
    }

    /// Gets network throughput in MB/s
    pub fn network_throughput_mbps(&self) -> f64 {
        (self.network_bytes_sent + self.network_bytes_received) as f64 / 1_048_576.0
    }
}

/// Error metrics by category and severity
#[derive(Default, Clone)]
pub struct ErrorMetrics {
    /// Total error count
    pub total_errors: u64,
    /// Error rate (errors per minute)
    pub error_rate: f64,
    /// Errors by HTTP status code
    pub errors_by_status: HashMap<u16, u64>,
    /// Critical errors requiring immediate attention
    pub critical_errors: u64,
    /// Last error timestamp
    pub last_error_time: Option<Instant>,
    /// Error recovery time (average time to resolve)
    pub average_recovery_time: Duration,
}

impl ErrorMetrics {
    /// Records an error occurrence
    pub fn record_error(&mut self, status_code: u16, is_critical: bool) {
        self.total_errors += 1;
        *self.errors_by_status.entry(status_code).or_insert(0) += 1;

        if is_critical {
            self.critical_errors += 1;
        }

        self.last_error_time = Some(Instant::now());
    }

    /// Updates error rate based on time window
    pub fn update_error_rate(&mut self, time_window_secs: u64) {
        if time_window_secs > 0 {
            let minutes = time_window_secs as f64 / 60.0;
            self.error_rate = self.total_errors as f64 / minutes;
        }
    }

    /// Gets error breakdown by status code
    pub fn error_breakdown(&self) -> Vec<(u16, u64)> {
        let mut breakdown: Vec<_> = self.errors_by_status.iter()
            .map(|(k, v)| (*k, *v))
            .collect();
        breakdown.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        breakdown
    }

    /// Gets time since last error in seconds
    pub fn time_since_last_error(&self) -> Option<u64> {
        self.last_error_time.map(|t| t.elapsed().as_secs())
    }
}

/// Flexible custom metric for domain-specific measurements
#[derive(Clone)]
pub struct CustomMetric {
    /// Metric name and description
    pub name: String,
    pub description: String,
    /// Metric type for proper aggregation
    pub metric_type: MetricType,
    /// Collected data points with timestamps
    pub data_points: Vec<(Instant, f64)>,
    /// Labels for metric dimensionality
    pub labels: HashMap<String, String>,
    /// Alert thresholds
    pub warning_threshold: Option<f64>,
    pub critical_threshold: Option<f64>,
}

impl CustomMetric {
    /// Creates a new custom metric
    pub fn new(
        name: String,
        description: String,
        metric_type: MetricType,
        labels: HashMap<String, String>,
        warning_threshold: Option<f64>,
        critical_threshold: Option<f64>,
    ) -> Self {
        Self {
            name,
            description,
            metric_type,
            data_points: Vec::new(),
            labels,
            warning_threshold,
            critical_threshold,
        }
    }

    /// Records a data point
    pub fn record(&mut self, value: f64) {
        self.data_points.push((Instant::now(), value));
        let one_hour_ago = Instant::now() - Duration::from_secs(3600);
        self.data_points.retain(|(t, _)| *t > one_hour_ago);
    }

    /// Gets current value based on metric type
    pub fn current_value(&self) -> f64 {
        match self.metric_type {
            MetricType::Counter => self.data_points.iter().map(|(_, v)| v).sum(),
            MetricType::Gauge => self.data_points.last().map(|(_, v)| *v).unwrap_or(0.0),
            MetricType::Histogram => {
                if self.data_points.is_empty() {
                    0.0
                } else {
                    let sum: f64 = self.data_points.iter().map(|(_, v)| v).sum();
                    sum / self.data_points.len() as f64
                }
            }
            MetricType::Rate => {
                let one_minute_ago = Instant::now() - Duration::from_secs(60);
                let recent_points: Vec<_> = self.data_points.iter()
                    .filter(|(t, _)| *t > one_minute_ago)
                    .collect();

                if recent_points.len() < 2 {
                    0.0
                } else {
                    let sum: f64 = recent_points.iter().map(|(_, v)| *v).sum();
                    sum / 60.0
                }
            }
        }
    }

    pub fn name(&self) -> &str { &self.name }
    pub fn description(&self) -> &str { &self.description }
    pub fn labels(&self) -> &HashMap<String, String> { &self.labels }
    pub fn warning_threshold(&self) -> Option<f64> { self.warning_threshold }
    pub fn critical_threshold(&self) -> Option<f64> { self.critical_threshold }
}

// ===== Metric Type Definitions =====

/// Supported metric types for proper aggregation strategies
#[derive(Clone, Debug, PartialEq)]
pub enum MetricType {
    /// Monotonically increasing counter (requests, bytes transferred)
    Counter,
    /// Value that can increase or decrease (queue size, active connections)
    Gauge,
    /// Duration measurements with percentile calculations
    Histogram,
    /// Rate of events over time windows
    Rate,
}

// ===== Summary Types for Export =====

/// Summary of a custom metric for export
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CustomMetricSummary {
    pub name: String,
    pub description: String,
    pub current_value: f64,
    pub labels: HashMap<String, String>,
    pub warning_threshold: Option<f64>,
    pub critical_threshold: Option<f64>,
}

/// Comprehensive metric snapshot for monitoring dashboards and alerting
#[derive(Serialize, Deserialize, Clone)]
pub struct MetricsSnapshot {
    pub timestamp: DateTime<Utc>,
    pub uptime_seconds: u64,
    pub http_summary: HttpSummary,
    pub database_summary: DatabaseSummary,
    pub business_summary: BusinessSummary,
    pub resource_summary: ResourceSummary,
    pub error_summary: ErrorSummary,
    pub health_indicators: HealthIndicators,
    pub active_alerts: Vec<Alert>,
}

/// HTTP metrics summary with key performance indicators
#[derive(Serialize, Deserialize, Clone)]
pub struct HttpSummary {
    pub total_requests: u64,
    pub requests_per_minute: f64,
    pub success_rate: f64,
    pub error_rate: f64,
    pub avg_response_time_ms: f64,
    pub p50_response_time_ms: f64,
    pub p95_response_time_ms: f64,
    pub p99_response_time_ms: f64,
    pub concurrent_requests: u32,
    pub slowest_endpoints: Vec<String>,
}

/// Database performance summary with optimization insights
#[derive(Serialize, Deserialize, Clone)]
pub struct DatabaseSummary {
    pub total_queries: u64,
    pub queries_per_second: f64,
    pub success_rate: f64,
    pub avg_execution_time_ms: f64,
    pub p95_execution_time_ms: f64,
    pub slow_query_count: u64,
    pub connection_pool_utilization: f64,
    pub deadlock_count: u64,
}

/// Business metrics summary for product insights
#[derive(Serialize, Deserialize, Clone)]
pub struct BusinessSummary {
    pub daily_active_users: u64,
    pub upload_volume_24h: u64,
    pub approval_rate: f64,
    pub engagement_rate: f64,
    pub cache_hit_rate: f64,
    pub ml_processing_success_rate: f64,
    pub moderation_backlog: u64,
    pub content_quality_score: f64,
}

/// Resource utilization summary for capacity planning
#[derive(Serialize, Deserialize, Clone)]
pub struct ResourceSummary {
    pub memory_usage_percent: f64,
    pub cpu_usage_percent: f64,
    pub database_connection_usage: f64,
    pub redis_memory_usage_mb: f64,
    pub storage_performance_score: f64,
    pub network_utilization: f64,
    pub disk_utilization: f64,
}

/// Error tracking summary
#[derive(Serialize, Deserialize, Clone)]
pub struct ErrorSummary {
    pub total_errors_24h: u64,
    pub error_rate: f64,
    pub critical_errors: u64,
    pub top_error_types: Vec<(String, u64)>,
    pub average_recovery_time_minutes: f64,
    pub errors_by_hour: Vec<u64>,
}

/// Overall service health indicators with detailed status
#[derive(Serialize, Deserialize, Clone)]
pub struct HealthIndicators {
    pub overall_health: HealthStatus,
    pub api_health: HealthStatus,
    pub database_health: HealthStatus,
    pub cache_health: HealthStatus,
    pub storage_health: HealthStatus,
    pub ml_service_health: HealthStatus,
    pub last_health_check: DateTime<Utc>,
}

/// Service health status levels with detailed context
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Critical,
}

/// Alert representation for monitoring systems
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Alert {
    pub id: String,
    pub severity: AlertSeverity,
    pub title: String,
    pub description: String,
    pub metric: String,
    pub threshold: f64,
    pub current_value: f64,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// Alert severity levels
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

// ===== Business Events =====

/// Business events that can be tracked for analytics
#[derive(Debug)]
pub enum BusinessEvent {
    UserActivity { user_id: Option<Uuid> },
    LetteringUploaded { country_code: String },
    LetteringApproved,
    LetteringRejected { reason: String },
    UserEngagement { engagement_type: EngagementType },
    ModerationCompleted { duration_hours: f64 },
    DuplicateDetected,
    CacheHit { cache_type: String },
    CacheMiss { cache_type: String },
    MlProcessingCompleted { success: bool, processing_time_ms: u64 },
}

/// Types of user engagement for analytics tracking
#[derive(Debug)]
pub enum EngagementType {
    Like,
    Comment,
    Report,
    Share,
    Download,
}
