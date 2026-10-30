//! Request counts and latency measured on this instance.

use axum::http::{Method, StatusCode};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use ts_rs::TS;

const LATENCY_UPPER_BOUNDS_MS: [u64; 8] = [1, 5, 10, 50, 100, 500, 1000, 5000];

#[derive(Default, Clone)]
struct Measurements {
    requests: u64,
    total_micros: u64,
    max_micros: u64,
    buckets: [u64; 8],
}

type Key = (String, &'static str, u16);

pub struct HttpMetrics {
    started: Instant,
    // Static router templates, seven methods + OTHER, five status classes + OTHER.
    // Labels contain no caller-controlled values.
    measurements: Mutex<BTreeMap<Key, Measurements>>,
}

#[derive(Debug, Serialize, TS)]
pub struct HttpMetric {
    pub route: String,
    pub method: &'static str,
    pub status_class: u16,
    #[ts(type = "number")]
    pub requests: u64,
    pub total_latency_ms: f64,
    pub max_latency_ms: f64,
    #[ts(type = "Array<number>")]
    pub latency_bucket_counts: [u64; 8],
}

#[derive(Debug, Serialize, TS)]
pub struct HttpMetricsSnapshot {
    pub scope: &'static str,
    #[ts(type = "number")]
    pub observation_seconds: u64,
    #[ts(type = "Array<number>")]
    pub latency_upper_bounds_ms: [u64; 8],
    pub routes: Vec<HttpMetric>,
}

impl Default for HttpMetrics {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            measurements: Mutex::new(BTreeMap::new()),
        }
    }
}

impl HttpMetrics {
    pub(crate) fn method(method: &Method) -> &'static str {
        match *method {
            Method::GET => "GET",
            Method::POST => "POST",
            Method::PUT => "PUT",
            Method::PATCH => "PATCH",
            Method::DELETE => "DELETE",
            Method::HEAD => "HEAD",
            Method::OPTIONS => "OPTIONS",
            _ => "OTHER",
        }
    }

    pub(crate) fn record(
        &self,
        route: &str,
        method: &'static str,
        status: StatusCode,
        duration: Duration,
    ) {
        let status_class = match status.as_u16() / 100 {
            class @ 1..=5 => class,
            _ => 0,
        };
        let mut values = self
            .measurements
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let value = values
            .entry((route.to_owned(), method, status_class))
            .or_default();
        let micros = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
        value.requests = value.requests.saturating_add(1);
        value.total_micros = value.total_micros.saturating_add(micros);
        value.max_micros = value.max_micros.max(micros);
        for (count, limit) in value.buckets.iter_mut().zip(LATENCY_UPPER_BOUNDS_MS) {
            if duration <= Duration::from_millis(limit) {
                *count = count.saturating_add(1);
            }
        }
    }

    pub fn snapshot(&self) -> HttpMetricsSnapshot {
        let values = self
            .measurements
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        HttpMetricsSnapshot {
            scope: "this_instance_since_metrics_start",
            observation_seconds: self.started.elapsed().as_secs(),
            latency_upper_bounds_ms: LATENCY_UPPER_BOUNDS_MS,
            routes: values
                .iter()
                .map(|((route, method, status_class), value)| HttpMetric {
                    route: route.clone(),
                    method,
                    status_class: *status_class,
                    requests: value.requests,
                    total_latency_ms: value.total_micros as f64 / 1000.0,
                    max_latency_ms: value.max_micros as f64 / 1000.0,
                    latency_bucket_counts: value.buckets,
                })
                .collect(),
        }
    }
}
