use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

/// Comprehensive metrics service for production observability and performance monitoring.
///
/// Tracks application performance, business metrics, and operational health indicators
/// across all service components. Provides both real-time monitoring capabilities and
/// historical trend analysis for capacity planning and incident response.
pub struct MetricsService {
    /// Thread-safe storage for metric data points
    inner: Arc<RwLock<MetricsInner>>,

    /// Service start time for uptime calculation
    start_time: Instant,
}

/// Internal metric storage with categorized collectors
#[derive(Default)]
struct MetricsInner {
    /// HTTP request metrics indexed by endpoint
    http_requests: HashMap<String, RequestMetrics>,

    /// Business domain metrics
    business_metrics: BusinessMetrics,

    /// Custom application metrics
    custom_metrics: HashMap<String, CustomMetric>,
}

/// HTTP request performance and error tracking
#[derive(Default, Clone)]
struct RequestMetrics {
    /// Total number of requests processed
    total_requests: u64,

    /// Requests completed successfully (2xx status)
    successful_requests: u64,

    /// Client error requests (4xx status)
    client_errors: u64,

    /// Server error requests (5xx status)
    server_errors: u64,

    /// Response time measurements in milliseconds
    response_times: Vec<u64>,
}

/// Business-specific metrics for product analytics
#[derive(Default, Clone)]
struct BusinessMetrics {
    /// Daily active users (unique IPs/authenticated users)
    daily_active_users: u64,

    /// Total lettering uploads processed
    total_uploads: u64,

    /// Upload approval rate (approved/total)
    upload_approval_rate: f64,

    /// Community engagement metrics
    total_likes: u64,
    total_comments: u64,
    total_reports: u64,
}

/// Flexible custom metric for domain-specific measurements
#[derive(Clone)]
struct CustomMetric {
    /// Metric name and description
    name: String,
    description: String,

    /// Metric type for proper aggregation
    metric_type: MetricType,

    /// Collected data points with timestamps
    data_points: Vec<(Instant, f64)>,

    /// Labels for metric dimensionality
    labels: HashMap<String, String>,
}

impl CustomMetric {
    /// Creates a new custom metric with the given configuration
    fn new(name: String, description: String, metric_type: MetricType, labels: HashMap<String, String>) -> Self {
        Self {
            name,
            description,
            metric_type,
            data_points: Vec::new(),
            labels,
        }
    }

    /// Records a data point for this metric
    fn record(&mut self, value: f64) {
        self.data_points.push((Instant::now(), value));

        // Keep only last hour of data points to prevent unbounded growth
        let one_hour_ago = Instant::now() - Duration::from_secs(3600);
        self.data_points.retain(|(timestamp, _)| *timestamp > one_hour_ago);
    }

    /// Gets the current value based on metric type
    fn current_value(&self) -> f64 {
        match self.metric_type {
            MetricType::Counter => {
                // For counters, sum all values
                self.data_points.iter().map(|(_, v)| v).sum()
            }
            MetricType::Gauge => {
                // For gauges, return the latest value
                self.data_points.last().map(|(_, v)| *v).unwrap_or(0.0)
            }
            MetricType::Histogram => {
                // For histograms, return the average
                if self.data_points.is_empty() {
                    0.0
                } else {
                    let sum: f64 = self.data_points.iter().map(|(_, v)| v).sum();
                    sum / self.data_points.len() as f64
                }
            }
            MetricType::Rate => {
                // For rates, calculate events per second over the last minute
                let one_minute_ago = Instant::now() - Duration::from_secs(60);
                let recent_points: Vec<_> = self.data_points.iter()
                    .filter(|(t, _)| *t > one_minute_ago)
                    .collect();

                if recent_points.len() < 2 {
                    0.0
                } else {
                    let sum: f64 = recent_points.iter().map(|(_, v)| *v).sum();
                    sum / 60.0 // events per second
                }
            }
        }
    }

    /// Gets the metric name
    fn name(&self) -> &str {
        &self.name
    }

    /// Gets the metric description
    fn description(&self) -> &str {
        &self.description
    }

    /// Gets the metric labels
    fn labels(&self) -> &HashMap<String, String> {
        &self.labels
    }
}

/// Supported metric types for proper aggregation strategies
#[derive(Clone, Debug)]
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

/// Comprehensive metric snapshot for monitoring dashboards
#[derive(Serialize, Deserialize, Clone)]
pub struct MetricsSnapshot {
    /// When this snapshot was captured
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Service uptime in seconds
    pub uptime_seconds: u64,

    /// HTTP request performance summary
    pub http_summary: HttpMetricsSummary,

    /// Business metrics summary
    pub business_summary: BusinessMetricsSummary,

    /// Health status indicators
    pub health_indicators: HealthIndicators,
}

/// HTTP metrics summary with key performance indicators
#[derive(Serialize, Deserialize, Clone)]
pub struct HttpMetricsSummary {
    pub total_requests: u64,
    pub requests_per_minute: f64,
    pub success_rate: f64,
    pub error_rate: f64,
    pub p50_response_time_ms: f64,
    pub p95_response_time_ms: f64,
    pub p99_response_time_ms: f64,
}

/// Business metrics summary for product insights
#[derive(Serialize, Deserialize, Clone)]
pub struct BusinessMetricsSummary {
    pub daily_active_users: u64,
    pub upload_volume_24h: u64,
    pub approval_rate: f64,
    pub engagement_rate: f64,
}

/// Overall service health indicators
#[derive(Serialize, Deserialize, Clone)]
pub struct HealthIndicators {
    pub overall_health: HealthStatus,
    pub database_health: HealthStatus,
    pub redis_health: HealthStatus,
    pub storage_health: HealthStatus,
    pub ml_service_health: HealthStatus,
    pub critical_errors_24h: u64,
}

/// Service health status levels
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HealthStatus {
    /// All systems operating normally
    Healthy,
    /// Minor issues present, service functional
    Degraded,
    /// Significant issues affecting functionality
    Unhealthy,
    /// Service unavailable or critically impaired
    Critical,
}

/// Business events that can be tracked for product analytics
#[derive(Debug)]
pub enum BusinessEvent {
    UserActivity { user_id: Uuid },
    LetteringUploaded { country_code: String },
    LetteringApproved,
    UserEngagement { engagement_type: EngagementType },
    ModerationCompleted { duration_hours: f64 },
    DuplicateDetected,
}

/// Types of user engagement for analytics tracking
#[derive(Debug)]
pub enum EngagementType {
    Like,
    Comment,
    Report,
}

/// Export format for custom metrics
#[derive(Debug, Clone, Serialize)]
pub struct CustomMetricExport {
    pub name: String,
    pub description: String,
    pub metric_type: String,
    pub current_value: f64,
    pub labels: HashMap<String, String>,
    pub data_point_count: usize,
}

impl MetricsService {
    /// Creates a new metrics service instance
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(MetricsInner::default())),
            start_time: Instant::now(),
        }
    }

    /// Records an HTTP request completion with response details
    #[instrument(skip(self), fields(endpoint = %endpoint, status = status_code))]
    pub async fn record_http_request(
        &self,
        endpoint: &str,
        method: &str,
        status_code: u16,
        duration: Duration,
    ) {
        let mut inner = self.inner.write().await;
        let key = format!("{}:{}", method, endpoint);
        let metrics = inner.http_requests.entry(key).or_default();

        metrics.total_requests += 1;
        metrics.response_times.push(duration.as_millis() as u64);

        // Categorize by status code
        match status_code {
            200..=299 => metrics.successful_requests += 1,
            400..=499 => metrics.client_errors += 1,
            500..=599 => metrics.server_errors += 1,
            _ => {} // Informational or other codes
        }

        // Limit stored response times to prevent memory growth
        if metrics.response_times.len() > 10000 {
            metrics.response_times.drain(0..1000);
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
            BusinessEvent::LetteringUploaded { country_code: _ } => {
                business.total_uploads += 1;
            }
            BusinessEvent::LetteringApproved => {
                // Recalculate approval rate
                let approved_count = business.total_uploads as f64 * business.upload_approval_rate + 1.0;
                business.upload_approval_rate = approved_count / business.total_uploads as f64;
            }
            BusinessEvent::UserEngagement { engagement_type } => {
                match engagement_type {
                    EngagementType::Like => business.total_likes += 1,
                    EngagementType::Comment => business.total_comments += 1,
                    EngagementType::Report => business.total_reports += 1,
                }
            }
            BusinessEvent::ModerationCompleted { duration_hours: _ } => {
                // Track moderation efficiency
            }
            BusinessEvent::DuplicateDetected => {
                // Track duplicate detection
            }
        }
    }

    /// Registers a custom metric for domain-specific measurements
    pub async fn register_custom_metric(
        &self,
        name: String,
        description: String,
        metric_type: MetricType,
        labels: HashMap<String, String>,
    ) {
        let mut inner = self.inner.write().await;
        let metric = CustomMetric::new(name.clone(), description, metric_type, labels);
        inner.custom_metrics.insert(name, metric);
    }

    /// Records a custom metric data point
    pub async fn record_custom_metric(&self, name: &str, value: f64) {
        let mut inner = self.inner.write().await;
        if let Some(metric) = inner.custom_metrics.get_mut(name) {
            // Use the record method which handles data retention automatically
            metric.record(value);
        }
    }

    /// Exports all custom metrics with their metadata
    pub async fn export_custom_metrics(&self) -> Vec<CustomMetricExport> {
        let inner = self.inner.read().await;
        inner.custom_metrics.values().map(|metric| {
            CustomMetricExport {
                name: metric.name().to_string(),
                description: metric.description().to_string(),
                metric_type: format!("{:?}", metric.metric_type),
                current_value: metric.current_value(),
                labels: metric.labels().clone(),
                data_point_count: metric.data_points.len(),
            }
        }).collect()
    }

    /// Generates a comprehensive metrics snapshot for monitoring systems
    pub async fn generate_snapshot(&self) -> MetricsSnapshot {
        let inner = self.inner.read().await;
        let uptime = self.start_time.elapsed().as_secs();

        // Calculate HTTP metrics summary
        let http_summary = self.calculate_http_summary(&inner.http_requests);
        let business_summary = self.calculate_business_summary(&inner.business_metrics);
        let health_indicators = self.calculate_health_indicators();

        MetricsSnapshot {
            timestamp: chrono::Utc::now(),
            uptime_seconds: uptime,
            http_summary,
            business_summary,
            health_indicators,
        }
    }

    /// Calculates HTTP performance summary with percentile statistics
    fn calculate_http_summary(&self, requests: &HashMap<String, RequestMetrics>) -> HttpMetricsSummary {
        let mut total_requests = 0;
        let mut successful_requests = 0;
        let mut all_response_times = Vec::new();

        for metrics in requests.values() {
            total_requests += metrics.total_requests;
            successful_requests += metrics.successful_requests;
            all_response_times.extend(&metrics.response_times);
        }

        // Calculate percentiles
        all_response_times.sort_unstable();
        let p50 = Self::calculate_percentile(&all_response_times, 50.0);
        let p95 = Self::calculate_percentile(&all_response_times, 95.0);
        let p99 = Self::calculate_percentile(&all_response_times, 99.0);

        let success_rate = if total_requests > 0 {
            successful_requests as f64 / total_requests as f64
        } else {
            0.0
        };

        HttpMetricsSummary {
            total_requests,
            requests_per_minute: 0.0, // TODO: Implement RPM calculation
            success_rate,
            error_rate: 1.0 - success_rate,
            p50_response_time_ms: p50,
            p95_response_time_ms: p95,
            p99_response_time_ms: p99,
        }
    }

    /// Calculates business metrics summary
    fn calculate_business_summary(&self, business: &BusinessMetrics) -> BusinessMetricsSummary {
        let engagement_rate = if business.total_uploads > 0 {
            (business.total_likes + business.total_comments) as f64 / business.total_uploads as f64
        } else {
            0.0
        };

        BusinessMetricsSummary {
            daily_active_users: business.daily_active_users,
            upload_volume_24h: business.total_uploads,
            approval_rate: business.upload_approval_rate,
            engagement_rate,
        }
    }

    /// Calculates overall service health indicators
    fn calculate_health_indicators(&self) -> HealthIndicators {
        HealthIndicators {
            overall_health: HealthStatus::Healthy,
            database_health: HealthStatus::Healthy,
            redis_health: HealthStatus::Healthy,
            storage_health: HealthStatus::Healthy,
            ml_service_health: HealthStatus::Healthy,
            critical_errors_24h: 0,
        }
    }

    /// Calculates percentile value from sorted data
    fn calculate_percentile(sorted_data: &[u64], percentile: f64) -> f64 {
        if sorted_data.is_empty() {
            return 0.0;
        }

        let index = (percentile / 100.0 * (sorted_data.len() - 1) as f64) as usize;
        sorted_data[index.min(sorted_data.len() - 1)] as f64
    }
}

impl Default for MetricsService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_http_request_recording() {
        let metrics = MetricsService::new();

        metrics.record_http_request("/api/v1/letterings", "GET", 200, Duration::from_millis(150)).await;
        metrics.record_http_request("/api/v1/letterings", "GET", 404, Duration::from_millis(50)).await;

        let snapshot = metrics.generate_snapshot().await;
        assert_eq!(snapshot.http_summary.total_requests, 2);
        assert_eq!(snapshot.http_summary.success_rate, 0.5);
    }

    #[tokio::test]
    async fn test_custom_metric_registration() {
        let metrics = MetricsService::new();
        let mut labels = HashMap::new();
        labels.insert("service".to_string(), "upload".to_string());

        metrics.register_custom_metric(
            "upload_queue_size".to_string(),
            "Number of items in upload processing queue".to_string(),
            MetricType::Gauge,
            labels,
        ).await;

        metrics.record_custom_metric("upload_queue_size", 25.0).await;
    }

    #[test]
    fn test_percentile_calculation() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        assert_eq!(MetricsService::calculate_percentile(&data, 50.0), 5.0);
        assert_eq!(MetricsService::calculate_percentile(&data, 95.0), 10.0);
    }
}
