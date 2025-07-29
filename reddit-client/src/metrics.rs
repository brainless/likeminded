use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub rate_limited_requests: u64,
    pub average_response_time: Duration,
    pub last_request_time: Option<SystemTime>,
    pub requests_by_endpoint: HashMap<String, EndpointMetrics>,
    pub hourly_request_counts: Vec<HourlyCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointMetrics {
    pub request_count: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub total_response_time: Duration,
    pub min_response_time: Duration,
    pub max_response_time: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyCount {
    pub timestamp: SystemTime,
    pub request_count: u64,
}

#[derive(Debug, Clone)]
pub struct RequestMetrics {
    pub endpoint: String,
    pub method: String,
    pub status_code: Option<u16>,
    pub response_time: Duration,
    pub success: bool,
    pub rate_limited: bool,
    pub error_type: Option<String>,
}

impl Default for ApiMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            rate_limited_requests: 0,
            average_response_time: Duration::from_millis(0),
            last_request_time: None,
            requests_by_endpoint: HashMap::new(),
            hourly_request_counts: Vec::new(),
        }
    }
}

impl EndpointMetrics {
    fn new() -> Self {
        Self {
            request_count: 0,
            success_count: 0,
            error_count: 0,
            total_response_time: Duration::from_millis(0),
            min_response_time: Duration::from_secs(u64::MAX),
            max_response_time: Duration::from_millis(0),
        }
    }

    fn update(&mut self, metrics: &RequestMetrics) {
        self.request_count += 1;
        self.total_response_time += metrics.response_time;

        if metrics.response_time < self.min_response_time {
            self.min_response_time = metrics.response_time;
        }
        if metrics.response_time > self.max_response_time {
            self.max_response_time = metrics.response_time;
        }

        if metrics.success {
            self.success_count += 1;
        } else {
            self.error_count += 1;
        }
    }

    pub fn average_response_time(&self) -> Duration {
        if self.request_count == 0 {
            Duration::from_millis(0)
        } else {
            self.total_response_time / self.request_count as u32
        }
    }

    pub fn success_rate(&self) -> f64 {
        if self.request_count == 0 {
            0.0
        } else {
            self.success_count as f64 / self.request_count as f64
        }
    }
}

#[derive(Debug)]
pub struct MetricsCollector {
    metrics: Arc<RwLock<ApiMetrics>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(ApiMetrics::default())),
        }
    }

    pub async fn record_request(&self, request_metrics: RequestMetrics) {
        let mut metrics = self.metrics.write().await;

        // Update global counters
        metrics.total_requests += 1;
        metrics.last_request_time = Some(SystemTime::now());

        if request_metrics.success {
            metrics.successful_requests += 1;
        } else {
            metrics.failed_requests += 1;
        }

        if request_metrics.rate_limited {
            metrics.rate_limited_requests += 1;
        }

        // Update average response time
        let total_time = metrics.average_response_time * metrics.total_requests as u32
            - metrics.average_response_time
            + request_metrics.response_time;
        metrics.average_response_time = total_time / metrics.total_requests as u32;

        // Update endpoint-specific metrics
        let endpoint_metrics = metrics
            .requests_by_endpoint
            .entry(request_metrics.endpoint.clone())
            .or_insert_with(EndpointMetrics::new);
        endpoint_metrics.update(&request_metrics);

        // Update hourly counts
        self.update_hourly_counts(&mut metrics).await;
    }

    async fn update_hourly_counts(&self, metrics: &mut ApiMetrics) {
        let now = SystemTime::now();
        let current_hour =
            now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() / 3600 * 3600; // Round down to hour
        let current_hour_time = UNIX_EPOCH + Duration::from_secs(current_hour);

        // Find or create current hour entry
        if let Some(last_entry) = metrics.hourly_request_counts.last_mut() {
            if last_entry.timestamp == current_hour_time {
                last_entry.request_count += 1;
            } else {
                metrics.hourly_request_counts.push(HourlyCount {
                    timestamp: current_hour_time,
                    request_count: 1,
                });
            }
        } else {
            metrics.hourly_request_counts.push(HourlyCount {
                timestamp: current_hour_time,
                request_count: 1,
            });
        }

        // Keep only last 24 hours
        let cutoff_time = now - Duration::from_secs(24 * 3600);
        metrics
            .hourly_request_counts
            .retain(|count| count.timestamp >= cutoff_time);
    }

    pub async fn get_metrics(&self) -> ApiMetrics {
        self.metrics.read().await.clone()
    }

    pub async fn get_endpoint_metrics(&self, endpoint: &str) -> Option<EndpointMetrics> {
        let metrics = self.metrics.read().await;
        metrics.requests_by_endpoint.get(endpoint).cloned()
    }

    pub async fn get_requests_per_minute(&self) -> f64 {
        let metrics = self.metrics.read().await;
        if let Some(last_request) = metrics.last_request_time {
            let elapsed = SystemTime::now()
                .duration_since(last_request)
                .unwrap_or_default();

            if elapsed < Duration::from_secs(60) {
                // Calculate based on recent activity
                let recent_requests = metrics
                    .hourly_request_counts
                    .iter()
                    .filter(|count| {
                        SystemTime::now()
                            .duration_since(count.timestamp)
                            .unwrap_or_default()
                            < Duration::from_secs(3600)
                    })
                    .map(|count| count.request_count)
                    .sum::<u64>();

                recent_requests as f64 / 60.0
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    pub async fn reset_metrics(&self) {
        let mut metrics = self.metrics.write().await;
        *metrics = ApiMetrics::default();
    }

    pub async fn export_metrics(&self) -> Result<String, serde_json::Error> {
        let metrics = self.get_metrics().await;
        serde_json::to_string_pretty(&metrics)
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collection() {
        let collector = MetricsCollector::new();

        let request_metrics = RequestMetrics {
            endpoint: "/api/v1/me".to_string(),
            method: "GET".to_string(),
            status_code: Some(200),
            response_time: Duration::from_millis(150),
            success: true,
            rate_limited: false,
            error_type: None,
        };

        collector.record_request(request_metrics).await;

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.total_requests, 1);
        assert_eq!(metrics.successful_requests, 1);
        assert_eq!(metrics.failed_requests, 0);
        assert!(metrics.last_request_time.is_some());
    }

    #[tokio::test]
    async fn test_endpoint_metrics() {
        let collector = MetricsCollector::new();

        let request_metrics = RequestMetrics {
            endpoint: "/api/v1/me".to_string(),
            method: "GET".to_string(),
            status_code: Some(200),
            response_time: Duration::from_millis(100),
            success: true,
            rate_limited: false,
            error_type: None,
        };

        collector.record_request(request_metrics).await;

        let endpoint_metrics = collector.get_endpoint_metrics("/api/v1/me").await;
        assert!(endpoint_metrics.is_some());

        let metrics = endpoint_metrics.unwrap();
        assert_eq!(metrics.request_count, 1);
        assert_eq!(metrics.success_count, 1);
        assert_eq!(metrics.average_response_time(), Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_export_metrics() {
        let collector = MetricsCollector::new();

        let request_metrics = RequestMetrics {
            endpoint: "/api/v1/me".to_string(),
            method: "GET".to_string(),
            status_code: Some(200),
            response_time: Duration::from_millis(150),
            success: true,
            rate_limited: false,
            error_type: None,
        };

        collector.record_request(request_metrics).await;

        let exported = collector.export_metrics().await;
        assert!(exported.is_ok());
        assert!(exported.unwrap().contains("total_requests"));
    }
}
