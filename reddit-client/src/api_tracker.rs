use crate::metrics::{MetricsCollector, RequestMetrics};
use crate::rate_limiter::RateLimitStatus;
use likeminded_core::CoreError;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCallRecord {
    pub id: Option<i64>,
    pub endpoint: String,
    pub method: String,
    pub status_code: Option<u16>,
    pub response_time_ms: i64,
    pub request_size_bytes: Option<i64>,
    pub response_size_bytes: Option<i64>,
    pub rate_limited: bool,
    pub retry_after_seconds: Option<i64>,
    pub error_type: Option<String>,
    pub user_agent: String,
    pub priority: i32,
    pub queue_wait_time_ms: i64,
    pub timestamp: i64,
    pub request_id: String,
    pub subreddit: Option<String>,
    pub operation_type: Option<String>,
    pub available_tokens_before: Option<i32>,
    pub available_tokens_after: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitWindow {
    pub id: Option<i64>,
    pub window_start: i64,
    pub window_end: i64,
    pub window_duration_seconds: i64,
    pub request_count: i64,
    pub successful_requests: i64,
    pub rate_limited_requests: i64,
    pub total_response_time_ms: i64,
    pub limit_reached: bool,
    pub max_requests_allowed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiUsageAlert {
    pub id: Option<i64>,
    pub alert_type: String,
    pub severity: String,
    pub message: String,
    pub threshold_value: Option<f64>,
    pub current_value: Option<f64>,
    pub endpoint: Option<String>,
    pub time_window_seconds: Option<i64>,
    pub triggered_at: i64,
    pub acknowledged_at: Option<i64>,
    pub resolved_at: Option<i64>,
    pub context_data: Option<String>,
    pub action_taken: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedRequest {
    pub id: Option<i64>,
    pub request_id: String,
    pub endpoint: String,
    pub method: String,
    pub priority: i32,
    pub operation_type: Option<String>,
    pub payload: Option<String>,
    pub headers: Option<String>,
    pub query_params: Option<String>,
    pub queued_at: i64,
    pub scheduled_for: Option<i64>,
    pub status: String,
    pub retry_count: i32,
    pub max_retries: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointConfig {
    pub id: Option<i64>,
    pub endpoint_pattern: String,
    pub rate_limit_per_minute: i64,
    pub rate_limit_per_hour: Option<i64>,
    pub priority_weight: f64,
    pub timeout_seconds: i64,
    pub max_retries: i64,
    pub description: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiUsageStats {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub rate_limited_requests: u64,
    pub average_response_time: Duration,
    pub requests_per_minute: f64,
    pub current_window_utilization: f64,
    pub endpoints_by_usage: Vec<(String, u64)>,
    pub active_alerts: Vec<ApiUsageAlert>,
    pub queue_size: usize,
    pub time_range: (SystemTime, SystemTime),
}

#[derive(Debug)]
pub struct ApiTracker {
    pool: Arc<SqlitePool>,
    metrics: Arc<MetricsCollector>,
    alert_thresholds: Arc<RwLock<AlertThresholds>>,
    endpoint_configs: Arc<RwLock<HashMap<String, EndpointConfig>>>,
}

#[derive(Debug, Clone)]
pub struct AlertThresholds {
    pub warning_utilization: f64,
    pub critical_utilization: f64,
    pub error_rate_threshold: f64,
    pub response_time_threshold: Duration,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            warning_utilization: 0.8,
            critical_utilization: 0.95,
            error_rate_threshold: 0.1,
            response_time_threshold: Duration::from_secs(5),
        }
    }
}

impl ApiTracker {
    pub fn new(pool: Arc<SqlitePool>, metrics: Arc<MetricsCollector>) -> Self {
        Self {
            pool,
            metrics,
            alert_thresholds: Arc::new(RwLock::new(AlertThresholds::default())),
            endpoint_configs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn initialize(&self) -> Result<(), CoreError> {
        self.load_endpoint_configs().await?;
        self.cleanup_old_data().await?;
        info!("API tracker initialized successfully");
        Ok(())
    }

    pub async fn record_api_call(
        &self,
        endpoint: &str,
        method: &str,
        status_code: Option<u16>,
        response_time: Duration,
        rate_limited: bool,
        priority: i32,
        queue_wait_time: Duration,
        operation_type: Option<&str>,
        subreddit: Option<&str>,
        tokens_before: Option<u32>,
        tokens_after: Option<u32>,
    ) -> Result<String, CoreError> {
        let request_id = Uuid::new_v4().to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let record = ApiCallRecord {
            id: None,
            endpoint: endpoint.to_string(),
            method: method.to_string(),
            status_code,
            response_time_ms: response_time.as_millis() as i64,
            request_size_bytes: None,
            response_size_bytes: None,
            rate_limited,
            retry_after_seconds: None,
            error_type: if status_code.map_or(false, |s| s >= 400) {
                Some(self.classify_error(status_code.unwrap()).to_string())
            } else {
                None
            },
            user_agent: "likeminded/1.0".to_string(), // TODO: Get from config
            priority,
            queue_wait_time_ms: queue_wait_time.as_millis() as i64,
            timestamp: now,
            request_id: request_id.clone(),
            subreddit: subreddit.map(|s| s.to_string()),
            operation_type: operation_type.map(|s| s.to_string()),
            available_tokens_before: tokens_before.map(|t| t as i32),
            available_tokens_after: tokens_after.map(|t| t as i32),
        };

        self.save_api_call_record(&record).await?;
        self.update_rate_limit_window(&record).await?;
        self.check_for_alerts(&record).await?;

        // Also record in metrics collector for compatibility
        let request_metrics = RequestMetrics {
            endpoint: endpoint.to_string(),
            method: method.to_string(),
            status_code,
            response_time,
            success: status_code.map_or(false, |s| s < 400),
            rate_limited,
            error_type: record.error_type.clone(),
        };
        self.metrics.record_request(request_metrics).await;

        debug!(
            "Recorded API call: {} {} ({}ms, rate_limited: {})",
            method,
            endpoint,
            response_time.as_millis(),
            rate_limited
        );

        Ok(request_id)
    }

    async fn save_api_call_record(&self, record: &ApiCallRecord) -> Result<(), CoreError> {
        sqlx::query!(
            r#"
            INSERT INTO api_call_tracking (
                endpoint, method, status_code, response_time_ms, rate_limited,
                error_type, user_agent, priority, queue_wait_time_ms, timestamp,
                request_id, subreddit, operation_type, available_tokens_before,
                available_tokens_after
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            record.endpoint,
            record.method,
            record.status_code,
            record.response_time_ms,
            record.rate_limited,
            record.error_type,
            record.user_agent,
            record.priority,
            record.queue_wait_time_ms,
            record.timestamp,
            record.request_id,
            record.subreddit,
            record.operation_type,
            record.available_tokens_before,
            record.available_tokens_after
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(())
    }

    async fn update_rate_limit_window(&self, record: &ApiCallRecord) -> Result<(), CoreError> {
        let window_duration = 60; // 1 minute window
        let window_start = (record.timestamp / window_duration) * window_duration;
        let window_end = window_start + window_duration;

        // Update or create window record
        let result = sqlx::query!(
            r#"
            INSERT INTO rate_limit_windows (
                window_start, window_end, window_duration_seconds,
                request_count, successful_requests, rate_limited_requests,
                total_response_time_ms, max_requests_allowed, created_at, updated_at
            ) VALUES (?, ?, ?, 1, ?, ?, ?, 100, ?, ?)
            ON CONFLICT(window_start, window_duration_seconds) DO UPDATE SET
                request_count = request_count + 1,
                successful_requests = successful_requests + ?,
                rate_limited_requests = rate_limited_requests + ?,
                total_response_time_ms = total_response_time_ms + ?,
                updated_at = ?
            "#,
            window_start,
            window_end,
            window_duration,
            if record.status_code.map_or(false, |s| s < 400) {
                1
            } else {
                0
            },
            if record.rate_limited { 1 } else { 0 },
            record.response_time_ms,
            record.timestamp,
            record.timestamp,
            if record.status_code.map_or(false, |s| s < 400) {
                1
            } else {
                0
            },
            if record.rate_limited { 1 } else { 0 },
            record.response_time_ms,
            record.timestamp
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(())
    }

    async fn check_for_alerts(&self, record: &ApiCallRecord) -> Result<(), CoreError> {
        let thresholds = self.alert_thresholds.read().await;

        // Check for rate limiting alerts
        if record.rate_limited {
            self.create_alert(
                "rate_limit_hit",
                "warning",
                "Request was rate limited",
                Some(1.0),
                Some(1.0),
                Some(&record.endpoint),
                Some(60),
                None,
            )
            .await?;
        }

        // Check response time alerts
        if record.response_time_ms > thresholds.response_time_threshold.as_millis() as i64 {
            self.create_alert(
                "slow_response",
                "warning",
                &format!("Slow response time: {}ms", record.response_time_ms),
                Some(thresholds.response_time_threshold.as_millis() as f64),
                Some(record.response_time_ms as f64),
                Some(&record.endpoint),
                None,
                None,
            )
            .await?;
        }

        Ok(())
    }

    async fn create_alert(
        &self,
        alert_type: &str,
        severity: &str,
        message: &str,
        threshold_value: Option<f64>,
        current_value: Option<f64>,
        endpoint: Option<&str>,
        time_window_seconds: Option<i64>,
        context_data: Option<&str>,
    ) -> Result<(), CoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        sqlx::query!(
            r#"
            INSERT INTO api_usage_alerts (
                alert_type, severity, message, threshold_value, current_value,
                endpoint, time_window_seconds, triggered_at, context_data
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            alert_type,
            severity,
            message,
            threshold_value,
            current_value,
            endpoint,
            time_window_seconds,
            now,
            context_data
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        warn!("API usage alert created: {} - {}", alert_type, message);
        Ok(())
    }

    pub async fn get_usage_stats(
        &self,
        time_range_hours: Option<u64>,
    ) -> Result<ApiUsageStats, CoreError> {
        let hours = time_range_hours.unwrap_or(24);
        let cutoff_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            - (hours as i64 * 3600);

        // Get basic stats
        let stats_row = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as total_requests,
                SUM(CASE WHEN status_code < 400 THEN 1 ELSE 0 END) as successful_requests,
                SUM(CASE WHEN status_code >= 400 THEN 1 ELSE 0 END) as failed_requests,
                SUM(CASE WHEN rate_limited THEN 1 ELSE 0 END) as rate_limited_requests,
                AVG(response_time_ms) as avg_response_time_ms
            FROM api_call_tracking 
            WHERE timestamp > ?
            "#,
            cutoff_time
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        // Get endpoint usage
        let endpoint_rows = sqlx::query!(
            r#"
            SELECT endpoint, COUNT(*) as count
            FROM api_call_tracking 
            WHERE timestamp > ?
            GROUP BY endpoint
            ORDER BY count DESC
            LIMIT 10
            "#,
            cutoff_time
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        // Get active alerts
        let alert_rows = sqlx::query!(
            r#"
            SELECT alert_type, severity, message, endpoint, triggered_at
            FROM api_usage_alerts 
            WHERE resolved_at IS NULL
            ORDER BY triggered_at DESC
            LIMIT 20
            "#
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        let alerts: Vec<ApiUsageAlert> = alert_rows
            .into_iter()
            .map(|row| ApiUsageAlert {
                id: None,
                alert_type: row.alert_type,
                severity: row.severity,
                message: row.message,
                threshold_value: None,
                current_value: None,
                endpoint: row.endpoint,
                time_window_seconds: None,
                triggered_at: row.triggered_at,
                acknowledged_at: None,
                resolved_at: None,
                context_data: None,
                action_taken: None,
            })
            .collect();

        // Calculate requests per minute
        let requests_per_minute = if hours > 0 {
            stats_row.total_requests as f64 / (hours as f64 * 60.0)
        } else {
            0.0
        };

        let endpoints_by_usage: Vec<(String, u64)> = endpoint_rows
            .into_iter()
            .map(|row| (row.endpoint, row.count as u64))
            .collect();

        Ok(ApiUsageStats {
            total_requests: stats_row.total_requests as u64,
            successful_requests: stats_row.successful_requests.unwrap_or(0) as u64,
            failed_requests: stats_row.failed_requests.unwrap_or(0) as u64,
            rate_limited_requests: stats_row.rate_limited_requests.unwrap_or(0) as u64,
            average_response_time: Duration::from_millis(
                stats_row.avg_response_time_ms.unwrap_or(0.0) as u64,
            ),
            requests_per_minute,
            current_window_utilization: 0.0, // TODO: Calculate current window utilization
            endpoints_by_usage,
            active_alerts: alerts,
            queue_size: 0, // TODO: Get actual queue size
            time_range: (
                SystemTime::UNIX_EPOCH + Duration::from_secs(cutoff_time as u64),
                SystemTime::now(),
            ),
        })
    }

    pub async fn acknowledge_alert(&self, alert_id: i64) -> Result<(), CoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        sqlx::query!(
            "UPDATE api_usage_alerts SET acknowledged_at = ? WHERE id = ?",
            now,
            alert_id
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(())
    }

    pub async fn resolve_alert(
        &self,
        alert_id: i64,
        action_taken: Option<&str>,
    ) -> Result<(), CoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        sqlx::query!(
            "UPDATE api_usage_alerts SET resolved_at = ?, action_taken = ? WHERE id = ?",
            now,
            action_taken,
            alert_id
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(())
    }

    async fn load_endpoint_configs(&self) -> Result<(), CoreError> {
        let rows = sqlx::query!(
            "SELECT endpoint_pattern, rate_limit_per_minute, priority_weight, timeout_seconds, max_retries, is_active FROM api_endpoint_configs"
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        let mut configs = self.endpoint_configs.write().await;
        for row in rows {
            if row.is_active {
                configs.insert(
                    row.endpoint_pattern.clone(),
                    EndpointConfig {
                        id: None,
                        endpoint_pattern: row.endpoint_pattern,
                        rate_limit_per_minute: row.rate_limit_per_minute,
                        rate_limit_per_hour: None,
                        priority_weight: row.priority_weight,
                        timeout_seconds: row.timeout_seconds,
                        max_retries: row.max_retries,
                        description: None,
                        is_active: row.is_active,
                    },
                );
            }
        }

        debug!("Loaded {} endpoint configurations", configs.len());
        Ok(())
    }

    async fn cleanup_old_data(&self) -> Result<(), CoreError> {
        let cutoff_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            - (30 * 24 * 3600); // 30 days

        // Clean up old API call records
        let deleted_calls = sqlx::query!(
            "DELETE FROM api_call_tracking WHERE timestamp < ?",
            cutoff_time
        )
        .execute(&*self.pool)
        .await
        .map_err(CoreError::Database)?
        .rows_affected();

        // Clean up old rate limit windows
        let deleted_windows = sqlx::query!(
            "DELETE FROM rate_limit_windows WHERE window_start < ?",
            cutoff_time
        )
        .execute(&*self.pool)
        .await
        .map_err(CoreError::Database)?
        .rows_affected();

        // Clean up resolved alerts older than 7 days
        let alert_cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            - (7 * 24 * 3600);

        let deleted_alerts = sqlx::query!(
            "DELETE FROM api_usage_alerts WHERE resolved_at IS NOT NULL AND resolved_at < ?",
            alert_cutoff
        )
        .execute(&*self.pool)
        .await
        .map_err(CoreError::Database)?
        .rows_affected();

        info!(
            "Cleaned up {} old API calls, {} old windows, {} old alerts",
            deleted_calls, deleted_windows, deleted_alerts
        );
        Ok(())
    }

    fn classify_error(&self, status_code: u16) -> &'static str {
        match status_code {
            401 => "unauthorized",
            403 => "forbidden",
            404 => "not_found",
            429 => "rate_limited",
            500..=599 => "server_error",
            _ => "client_error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_error_classification() {
        let pool = Arc::new(sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap());
        let metrics = Arc::new(MetricsCollector::new());
        let tracker = ApiTracker::new(pool, metrics);

        assert_eq!(tracker.classify_error(401), "unauthorized");
        assert_eq!(tracker.classify_error(429), "rate_limited");
        assert_eq!(tracker.classify_error(404), "not_found");
        assert_eq!(tracker.classify_error(500), "server_error");
    }

    #[test]
    fn test_api_call_record_creation() {
        let record = ApiCallRecord {
            id: None,
            endpoint: "/api/v1/me".to_string(),
            method: "GET".to_string(),
            status_code: Some(200),
            response_time_ms: 150,
            request_size_bytes: None,
            response_size_bytes: None,
            rate_limited: false,
            retry_after_seconds: None,
            error_type: None,
            user_agent: "test-agent".to_string(),
            priority: 0,
            queue_wait_time_ms: 50,
            timestamp: 1640995200,
            request_id: "test-123".to_string(),
            subreddit: None,
            operation_type: Some("get_user_info".to_string()),
            available_tokens_before: Some(10),
            available_tokens_after: Some(9),
        };

        assert_eq!(record.endpoint, "/api/v1/me");
        assert_eq!(record.method, "GET");
        assert_eq!(record.status_code, Some(200));
        assert!(!record.rate_limited);
    }

    #[test]
    fn test_alert_thresholds() {
        let thresholds = AlertThresholds::default();
        assert_eq!(thresholds.warning_utilization, 0.8);
        assert_eq!(thresholds.critical_utilization, 0.95);
        assert_eq!(thresholds.error_rate_threshold, 0.1);
        assert_eq!(thresholds.response_time_threshold, Duration::from_secs(5));
    }

    #[test]
    fn test_rate_limit_window() {
        let window = RateLimitWindow {
            id: None,
            window_start: 1640995200,
            window_end: 1640995260,
            window_duration_seconds: 60,
            request_count: 5,
            successful_requests: 4,
            rate_limited_requests: 1,
            total_response_time_ms: 750,
            limit_reached: false,
            max_requests_allowed: 100,
        };

        assert_eq!(window.window_duration_seconds, 60);
        assert_eq!(window.request_count, 5);
        assert!(!window.limit_reached);
    }

    #[test]
    fn test_api_usage_alert() {
        let alert = ApiUsageAlert {
            id: None,
            alert_type: "rate_limit_approaching".to_string(),
            severity: "warning".to_string(),
            message: "Rate limit utilization at 85%".to_string(),
            threshold_value: Some(0.8),
            current_value: Some(0.85),
            endpoint: Some("/api/v1/me".to_string()),
            time_window_seconds: Some(60),
            triggered_at: 1640995200,
            acknowledged_at: None,
            resolved_at: None,
            context_data: None,
            action_taken: None,
        };

        assert_eq!(alert.alert_type, "rate_limit_approaching");
        assert_eq!(alert.severity, "warning");
        assert_eq!(alert.threshold_value, Some(0.8));
    }
}
