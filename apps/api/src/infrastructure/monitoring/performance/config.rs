//! Configuration for the performance monitoring system.

/// Configuration for monitoring behavior and thresholds
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// Maximum data points to retain per metric
    pub max_data_points: usize,

    /// Slow query threshold in milliseconds
    pub slow_query_threshold_ms: u64,

    /// High response time threshold in milliseconds
    pub high_response_time_threshold_ms: u64,

    /// Error rate threshold for alerting (errors per minute)
    pub error_rate_threshold: f64,

    /// Memory usage threshold percentage
    pub memory_usage_threshold_percent: f64,

    /// CPU usage threshold percentage
    pub cpu_usage_threshold_percent: f64,

    /// Enable automatic cleanup of old metrics
    pub enable_automatic_cleanup: bool,

    /// Cleanup interval in minutes
    pub cleanup_interval_minutes: u64,
}

impl MonitorConfig {
    /// Checks if error rate exceeds configured threshold
    pub fn is_error_rate_critical(&self, current_error_rate: f64) -> bool {
        current_error_rate > self.error_rate_threshold
    }

    /// Gets error rate threshold
    pub fn error_rate_threshold(&self) -> f64 {
        self.error_rate_threshold
    }
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            max_data_points: 10000,
            slow_query_threshold_ms: 1000,
            high_response_time_threshold_ms: 5000,
            error_rate_threshold: 10.0,
            memory_usage_threshold_percent: 85.0,
            cpu_usage_threshold_percent: 80.0,
            enable_automatic_cleanup: true,
            cleanup_interval_minutes: 60,
        }
    }
}
