//! Monitoring and observability infrastructure for production deployments.
//!
//! This module provides comprehensive monitoring capabilities including:
//! - Performance metrics collection and aggregation
//! - Health check endpoints and status monitoring
//! - Request tracing and distributed logging
//! - Business metrics for product analytics
//! - Resource utilization tracking
//!
//! The monitoring system is designed to be lightweight, thread-safe, and
//! suitable for high-throughput production environments.

pub mod metrics;
pub mod performance;

pub use metrics::{
    MetricsService, MetricsSnapshot, HealthStatus, BusinessEvent,
    EngagementType, MetricType
};

pub use performance::{
    PerformanceMonitor, MonitorConfig, Alert, AlertSeverity,
    HttpSummary, DatabaseSummary, BusinessSummary, ResourceSummary,
    HealthIndicators
};

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

/// Centralized monitoring coordinator that manages all observability components.
///
/// Provides a unified interface for collecting metrics, health checks, and
/// system monitoring across all application layers.
pub struct MonitoringService {
    /// Core metrics collection service
    pub metrics: Arc<MetricsService>,

    /// Performance monitoring service
    pub performance: Arc<PerformanceMonitor>,

    /// Health check registry for service dependencies
    health_checks: Arc<RwLock<Vec<Box<dyn HealthCheck + Send + Sync>>>>,
}

/// Health check trait for monitoring service dependencies.
///
/// Implement this trait for external services (database, Redis, storage)
/// to enable automated health monitoring and alerting.
#[async_trait::async_trait]
pub trait HealthCheck {
    /// Returns the name of this health check for identification
    fn name(&self) -> &str;

    /// Performs the health check and returns the current status
    async fn check(&self) -> HealthCheckResult;

    /// Returns the timeout duration for this health check
    fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(10)
    }
}

/// Result of a health check operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Whether the service is healthy
    pub healthy: bool,

    /// Optional message providing additional context
    pub message: Option<String>,

    /// Response time for the health check
    pub response_time_ms: u64,

    /// Additional metadata about the service state
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl MonitoringService {
    /// Creates a new monitoring service with default configuration
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(MetricsService::new()),
            performance: Arc::new(PerformanceMonitor::new()),
            health_checks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Registers a health check for a service dependency
    pub async fn register_health_check(&self, check: Box<dyn HealthCheck + Send + Sync>) {
        let mut checks = self.health_checks.write().await;
        checks.push(check);
    }

    /// Performs all registered health checks and returns overall status
    pub async fn check_health(&self) -> OverallHealthStatus {
        let checks = self.health_checks.read().await;
        let mut results = Vec::new();
        let mut all_healthy = true;

        for check in checks.iter() {
            let _start_time = std::time::Instant::now();

            let result = match tokio::time::timeout(check.timeout(), check.check()).await {
                Ok(result) => result,
                Err(_) => HealthCheckResult {
                    healthy: false,
                    message: Some("Health check timed out".to_string()),
                    response_time_ms: check.timeout().as_millis() as u64,
                    metadata: std::collections::HashMap::new(),
                },
            };

            if !result.healthy {
                all_healthy = false;
            }

            results.push((check.name().to_string(), result));
        }

        OverallHealthStatus {
            healthy: all_healthy,
            checks: results,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Overall health status for all monitored services
#[derive(Debug, Serialize, Deserialize)]
pub struct OverallHealthStatus {
    /// Whether all services are healthy
    pub healthy: bool,

    /// Individual health check results
    pub checks: Vec<(String, HealthCheckResult)>,

    /// When this health check was performed
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Database health check implementation
pub struct DatabaseHealthCheck {
    pool: sqlx::PgPool,
}

impl DatabaseHealthCheck {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl HealthCheck for DatabaseHealthCheck {
    fn name(&self) -> &str {
        "database"
    }

    async fn check(&self) -> HealthCheckResult {
        let start_time = std::time::Instant::now();

        match sqlx::query("SELECT 1").fetch_one(&self.pool).await {
            Ok(_) => {
                let mut metadata = std::collections::HashMap::new();

                // Get pool statistics
                metadata.insert(
                    "active_connections".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(self.pool.size()))
                );

                HealthCheckResult {
                    healthy: true,
                    message: Some("Database connection successful".to_string()),
                    response_time_ms: start_time.elapsed().as_millis() as u64,
                    metadata,
                }
            }
            Err(e) => HealthCheckResult {
                healthy: false,
                message: Some(format!("Database connection failed: {}", e)),
                response_time_ms: start_time.elapsed().as_millis() as u64,
                metadata: std::collections::HashMap::new(),
            },
        }
    }
}

/// Redis health check implementation
pub struct RedisHealthCheck {
    client: redis::Client,
}

impl RedisHealthCheck {
    pub fn new(client: redis::Client) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl HealthCheck for RedisHealthCheck {
    fn name(&self) -> &str {
        "redis"
    }

    async fn check(&self) -> HealthCheckResult {
        let start_time = std::time::Instant::now();

        match self.client.get_multiplexed_async_connection().await {
            Ok(mut conn) => {
                match redis::cmd("PING").query_async::<String>(&mut conn).await {
                    Ok(response) => {
                        let healthy = response == "PONG";
                        HealthCheckResult {
                            healthy,
                            message: Some(format!("Redis ping response: {}", response)),
                            response_time_ms: start_time.elapsed().as_millis() as u64,
                            metadata: std::collections::HashMap::new(),
                        }
                    }
                    Err(e) => HealthCheckResult {
                        healthy: false,
                        message: Some(format!("Redis ping failed: {}", e)),
                        response_time_ms: start_time.elapsed().as_millis() as u64,
                        metadata: std::collections::HashMap::new(),
                    },
                }
            }
            Err(e) => HealthCheckResult {
                healthy: false,
                message: Some(format!("Redis connection failed: {}", e)),
                response_time_ms: start_time.elapsed().as_millis() as u64,
                metadata: std::collections::HashMap::new(),
            },
        }
    }
}

impl Default for MonitoringService {
    fn default() -> Self {
        Self::new()
    }
}
