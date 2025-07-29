use crate::api_tracker::{ApiTracker, ApiUsageStats};
use crate::request_queue::{QueueStats, RequestQueue};
use likeminded_core::CoreError;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub overview: OverviewStats,
    pub rate_limits: RateLimitInfo,
    pub endpoints: Vec<EndpointStats>,
    pub alerts: Vec<AlertInfo>,
    pub queue: QueueInfo,
    pub performance: PerformanceMetrics,
    pub usage_trends: UsageTrends,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewStats {
    pub total_requests_today: u64,
    pub successful_requests_today: u64,
    pub failed_requests_today: u64,
    pub rate_limited_requests_today: u64,
    pub success_rate_percentage: f64,
    pub average_response_time: Duration,
    pub requests_per_minute_current: f64,
    pub requests_per_minute_peak: f64,
    pub uptime: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitInfo {
    pub current_utilization_percentage: f64,
    pub available_tokens: u32,
    pub max_tokens: u32,
    pub requests_in_current_window: u32,
    pub max_requests_per_window: u32,
    pub time_until_window_reset: Duration,
    pub estimated_wait_for_next_request: Option<Duration>,
    pub is_near_limit: bool,
    pub is_at_limit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointStats {
    pub endpoint_pattern: String,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub rate_limited_requests: u64,
    pub average_response_time: Duration,
    pub min_response_time: Duration,
    pub max_response_time: Duration,
    pub success_rate_percentage: f64,
    pub requests_per_minute: f64,
    pub last_request_time: Option<SystemTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertInfo {
    pub id: i64,
    pub alert_type: String,
    pub severity: String,
    pub message: String,
    pub endpoint: Option<String>,
    pub triggered_at: SystemTime,
    pub is_acknowledged: bool,
    pub is_resolved: bool,
    pub time_since_triggered: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueInfo {
    pub total_queued: usize,
    pub high_priority_queued: usize,
    pub normal_priority_queued: usize,
    pub low_priority_queued: usize,
    pub currently_executing: usize,
    pub completed_today: u64,
    pub failed_today: u64,
    pub average_queue_wait_time: Duration,
    pub longest_waiting_request: Option<Duration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub p50_response_time: Duration,
    pub p95_response_time: Duration,
    pub p99_response_time: Duration,
    pub slowest_endpoints: Vec<(String, Duration)>,
    pub fastest_endpoints: Vec<(String, Duration)>,
    pub error_rate_by_endpoint: Vec<(String, f64)>,
    pub throughput_trend: Vec<(SystemTime, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageTrends {
    pub hourly_request_counts: Vec<(SystemTime, u64)>,
    pub daily_request_counts: Vec<(SystemTime, u64)>,
    pub success_rate_trend: Vec<(SystemTime, f64)>,
    pub response_time_trend: Vec<(SystemTime, Duration)>,
}

#[derive(Debug)]
pub struct UsageDashboard {
    pool: Arc<SqlitePool>,
    api_tracker: Option<Arc<ApiTracker>>,
    request_queue: Option<Arc<RequestQueue>>,
    cache: Arc<RwLock<Option<(DashboardData, SystemTime)>>>,
    cache_ttl: Duration,
}

impl UsageDashboard {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self {
            pool,
            api_tracker: None,
            request_queue: None,
            cache: Arc::new(RwLock::new(None)),
            cache_ttl: Duration::from_secs(30), // Cache for 30 seconds
        }
    }

    pub fn with_api_tracker(mut self, api_tracker: Arc<ApiTracker>) -> Self {
        self.api_tracker = Some(api_tracker);
        self
    }

    pub fn with_request_queue(mut self, request_queue: Arc<RequestQueue>) -> Self {
        self.request_queue = Some(request_queue);
        self
    }

    pub async fn get_dashboard_data(
        &self,
        force_refresh: bool,
    ) -> Result<DashboardData, CoreError> {
        // Check cache first
        if !force_refresh {
            let cache = self.cache.read().await;
            if let Some((data, cached_at)) = cache.as_ref() {
                if cached_at.elapsed().unwrap_or_default() < self.cache_ttl {
                    debug!("Returning cached dashboard data");
                    return Ok(data.clone());
                }
            }
        }

        debug!("Generating fresh dashboard data");
        let dashboard_data = self.generate_dashboard_data().await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            *cache = Some((dashboard_data.clone(), SystemTime::now()));
        }

        Ok(dashboard_data)
    }

    async fn generate_dashboard_data(&self) -> Result<DashboardData, CoreError> {
        let now = SystemTime::now();

        // Generate all sections concurrently
        let (overview, rate_limits, endpoints, alerts, queue, performance, trends) = tokio::join!(
            self.generate_overview_stats(),
            self.generate_rate_limit_info(),
            self.generate_endpoint_stats(),
            self.generate_alert_info(),
            self.generate_queue_info(),
            self.generate_performance_metrics(),
            self.generate_usage_trends()
        );

        Ok(DashboardData {
            overview: overview?,
            rate_limits: rate_limits?,
            endpoints: endpoints?,
            alerts: alerts?,
            queue: queue?,
            performance: performance?,
            usage_trends: trends?,
            timestamp: now,
        })
    }

    async fn generate_overview_stats(&self) -> Result<OverviewStats, CoreError> {
        let today_start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            - (24 * 3600);

        let stats_row = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as total_requests,
                SUM(CASE WHEN status_code IS NOT NULL AND status_code < 400 THEN 1 ELSE 0 END) as successful_requests,
                SUM(CASE WHEN status_code IS NOT NULL AND status_code >= 400 THEN 1 ELSE 0 END) as failed_requests,
                SUM(CASE WHEN rate_limited THEN 1 ELSE 0 END) as rate_limited_requests,
                AVG(response_time_ms) as avg_response_time_ms,
                MIN(timestamp) as earliest_request
            FROM api_call_tracking 
            WHERE timestamp > ?
            "#,
            today_start
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        let total = stats_row.total_requests as f64;
        let successful = stats_row.successful_requests.unwrap_or(0) as f64;
        let success_rate = if total > 0.0 {
            (successful / total) * 100.0
        } else {
            0.0
        };

        let requests_per_minute = total / (24.0 * 60.0); // Average over 24 hours

        // Calculate uptime based on earliest request
        let uptime = if let Some(earliest) = stats_row.earliest_request {
            Duration::from_secs(
                (SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    - earliest as u64),
            )
        } else {
            Duration::from_secs(0)
        };

        // Get peak requests per minute from last hour
        let peak_rpm = self.get_peak_requests_per_minute().await?;

        Ok(OverviewStats {
            total_requests_today: stats_row.total_requests as u64,
            successful_requests_today: stats_row.successful_requests.unwrap_or(0) as u64,
            failed_requests_today: stats_row.failed_requests.unwrap_or(0) as u64,
            rate_limited_requests_today: stats_row.rate_limited_requests.unwrap_or(0) as u64,
            success_rate_percentage: success_rate,
            average_response_time: Duration::from_millis(
                stats_row.avg_response_time_ms.unwrap_or(0.0) as u64,
            ),
            requests_per_minute_current: requests_per_minute,
            requests_per_minute_peak: peak_rpm,
            uptime,
        })
    }

    async fn get_peak_requests_per_minute(&self) -> Result<f64, CoreError> {
        let one_hour_ago = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            - 3600;

        let peak_row = sqlx::query!(
            r#"
            SELECT MAX(request_count) as peak_requests
            FROM rate_limit_windows
            WHERE window_start > ? AND window_duration_seconds = 60
            "#,
            one_hour_ago
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(peak_row.peak_requests.unwrap_or(0) as f64)
    }

    async fn generate_rate_limit_info(&self) -> Result<RateLimitInfo, CoreError> {
        // This would typically come from the rate limiter, but we'll simulate it
        // In a real implementation, you'd get this from the actual rate limiter

        let current_window = self.get_current_window_stats().await?;
        let max_requests = 100; // Reddit's limit
        let utilization = (current_window.request_count as f64 / max_requests as f64) * 100.0;

        Ok(RateLimitInfo {
            current_utilization_percentage: utilization,
            available_tokens: (max_requests - current_window.request_count).max(0) as u32,
            max_tokens: max_requests as u32,
            requests_in_current_window: current_window.request_count as u32,
            max_requests_per_window: max_requests as u32,
            time_until_window_reset: current_window.time_until_reset,
            estimated_wait_for_next_request: if current_window.request_count >= max_requests {
                Some(Duration::from_secs(60))
            } else {
                None
            },
            is_near_limit: utilization > 80.0,
            is_at_limit: current_window.request_count >= max_requests,
        })
    }

    async fn get_current_window_stats(&self) -> Result<CurrentWindowStats, CoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let window_start = (now / 60) * 60; // Current minute window

        let window_row = sqlx::query!(
            r#"
            SELECT request_count, window_start
            FROM rate_limit_windows
            WHERE window_start = ? AND window_duration_seconds = 60
            "#,
            window_start
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        let request_count = window_row.map(|r| r.request_count).unwrap_or(0);
        let seconds_into_window = now % 60;
        let time_until_reset = Duration::from_secs(60 - seconds_into_window);

        Ok(CurrentWindowStats {
            request_count,
            time_until_reset,
        })
    }

    async fn generate_endpoint_stats(&self) -> Result<Vec<EndpointStats>, CoreError> {
        let one_day_ago = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            - (24 * 3600);

        let endpoint_rows = sqlx::query!(
            r#"
            SELECT 
                endpoint,
                COUNT(*) as total_requests,
                SUM(CASE WHEN status_code IS NOT NULL AND status_code < 400 THEN 1 ELSE 0 END) as successful_requests,
                SUM(CASE WHEN status_code IS NOT NULL AND status_code >= 400 THEN 1 ELSE 0 END) as failed_requests,
                SUM(CASE WHEN rate_limited THEN 1 ELSE 0 END) as rate_limited_requests,
                AVG(response_time_ms) as avg_response_time_ms,
                MIN(response_time_ms) as min_response_time_ms,
                MAX(response_time_ms) as max_response_time_ms,
                MAX(timestamp) as last_request_timestamp
            FROM api_call_tracking 
            WHERE timestamp > ?
            GROUP BY endpoint
            ORDER BY total_requests DESC
            LIMIT 20
            "#,
            one_day_ago
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        let mut endpoint_stats = Vec::new();
        for row in endpoint_rows {
            let total = row.total_requests as f64;
            let successful = row.successful_requests.unwrap_or(0) as f64;
            let success_rate = if total > 0.0 {
                (successful / total) * 100.0
            } else {
                0.0
            };
            let requests_per_minute = total / (24.0 * 60.0);

            let last_request_time = row
                .last_request_timestamp
                .map(|ts| SystemTime::UNIX_EPOCH + Duration::from_secs(ts as u64));

            endpoint_stats.push(EndpointStats {
                endpoint_pattern: row.endpoint,
                total_requests: row.total_requests as u64,
                successful_requests: row.successful_requests.unwrap_or(0) as u64,
                failed_requests: row.failed_requests.unwrap_or(0) as u64,
                rate_limited_requests: row.rate_limited_requests.unwrap_or(0) as u64,
                average_response_time: Duration::from_millis(
                    row.avg_response_time_ms.unwrap_or(0.0) as u64,
                ),
                min_response_time: Duration::from_millis(
                    row.min_response_time_ms.unwrap_or(0) as u64
                ),
                max_response_time: Duration::from_millis(
                    row.max_response_time_ms.unwrap_or(0) as u64
                ),
                success_rate_percentage: success_rate,
                requests_per_minute,
                last_request_time,
            });
        }

        Ok(endpoint_stats)
    }

    async fn generate_alert_info(&self) -> Result<Vec<AlertInfo>, CoreError> {
        let alert_rows = sqlx::query!(
            r#"
            SELECT id, alert_type, severity, message, endpoint, triggered_at,
                   acknowledged_at, resolved_at
            FROM api_usage_alerts 
            WHERE triggered_at > ? OR resolved_at IS NULL
            ORDER BY triggered_at DESC
            LIMIT 50
            "#,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                - (7 * 24 * 3600) // Last 7 days
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        let mut alerts = Vec::new();
        for row in alert_rows {
            let triggered_at =
                SystemTime::UNIX_EPOCH + Duration::from_secs(row.triggered_at as u64);
            let time_since_triggered = SystemTime::now()
                .duration_since(triggered_at)
                .unwrap_or_default();

            alerts.push(AlertInfo {
                id: row.id,
                alert_type: row.alert_type,
                severity: row.severity,
                message: row.message,
                endpoint: row.endpoint,
                triggered_at,
                is_acknowledged: row.acknowledged_at.is_some(),
                is_resolved: row.resolved_at.is_some(),
                time_since_triggered,
            });
        }

        Ok(alerts)
    }

    async fn generate_queue_info(&self) -> Result<QueueInfo, CoreError> {
        if let Some(ref queue) = self.request_queue {
            let queue_stats = queue.get_queue_stats().await?;

            // Get priority breakdown from database
            let priority_rows = sqlx::query!(
                r#"
                SELECT priority, COUNT(*) as count
                FROM request_queue
                WHERE status = 'queued'
                GROUP BY priority
                "#
            )
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

            let mut high_priority = 0;
            let mut normal_priority = 0;
            let mut low_priority = 0;

            for row in priority_rows {
                match row.priority {
                    1 => high_priority = row.count as usize,
                    0 => normal_priority = row.count as usize,
                    -1 => low_priority = row.count as usize,
                    _ => {}
                }
            }

            Ok(QueueInfo {
                total_queued: queue_stats.total_queued,
                high_priority_queued: high_priority,
                normal_priority_queued: normal_priority,
                low_priority_queued: low_priority,
                currently_executing: queue_stats.executing,
                completed_today: queue_stats.completed_today,
                failed_today: queue_stats.failed_today,
                average_queue_wait_time: queue_stats.average_wait_time,
                longest_waiting_request: None, // TODO: Calculate from database
            })
        } else {
            Ok(QueueInfo {
                total_queued: 0,
                high_priority_queued: 0,
                normal_priority_queued: 0,
                low_priority_queued: 0,
                currently_executing: 0,
                completed_today: 0,
                failed_today: 0,
                average_queue_wait_time: Duration::from_secs(0),
                longest_waiting_request: None,
            })
        }
    }

    async fn generate_performance_metrics(&self) -> Result<PerformanceMetrics, CoreError> {
        // Get percentile response times
        let percentile_rows = sqlx::query!(
            r#"
            SELECT response_time_ms
            FROM api_call_tracking
            WHERE timestamp > ? AND status_code IS NOT NULL
            ORDER BY response_time_ms
            "#,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                - (24 * 3600)
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        let response_times: Vec<u64> = percentile_rows
            .into_iter()
            .map(|row| row.response_time_ms as u64)
            .collect();

        let (p50, p95, p99) = calculate_percentiles(&response_times);

        // Get slowest and fastest endpoints
        let (slowest, fastest) = self.get_endpoint_speed_rankings().await?;

        // Get error rates by endpoint
        let error_rates = self.get_error_rates_by_endpoint().await?;

        // Get throughput trend (simplified)
        let throughput_trend = self.get_throughput_trend().await?;

        Ok(PerformanceMetrics {
            p50_response_time: Duration::from_millis(p50),
            p95_response_time: Duration::from_millis(p95),
            p99_response_time: Duration::from_millis(p99),
            slowest_endpoints: slowest,
            fastest_endpoints: fastest,
            error_rate_by_endpoint: error_rates,
            throughput_trend,
        })
    }

    async fn get_endpoint_speed_rankings(
        &self,
    ) -> Result<(Vec<(String, Duration)>, Vec<(String, Duration)>), CoreError> {
        let speed_rows = sqlx::query!(
            r#"
            SELECT endpoint, AVG(response_time_ms) as avg_response_time
            FROM api_call_tracking
            WHERE timestamp > ? AND status_code IS NOT NULL
            GROUP BY endpoint
            HAVING COUNT(*) >= 5
            ORDER BY avg_response_time DESC
            "#,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                - (24 * 3600)
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        let slowest: Vec<(String, Duration)> = speed_rows
            .iter()
            .take(5)
            .map(|row| {
                (
                    row.endpoint.clone(),
                    Duration::from_millis(row.avg_response_time.unwrap_or(0.0) as u64),
                )
            })
            .collect();

        let fastest: Vec<(String, Duration)> = speed_rows
            .iter()
            .rev()
            .take(5)
            .map(|row| {
                (
                    row.endpoint.clone(),
                    Duration::from_millis(row.avg_response_time.unwrap_or(0.0) as u64),
                )
            })
            .collect();

        Ok((slowest, fastest))
    }

    async fn get_error_rates_by_endpoint(&self) -> Result<Vec<(String, f64)>, CoreError> {
        let error_rows = sqlx::query!(
            r#"
            SELECT 
                endpoint,
                COUNT(*) as total_requests,
                SUM(CASE WHEN status_code >= 400 THEN 1 ELSE 0 END) as error_requests
            FROM api_call_tracking
            WHERE timestamp > ? AND status_code IS NOT NULL
            GROUP BY endpoint
            HAVING total_requests >= 10
            ORDER BY (error_requests * 1.0 / total_requests) DESC
            LIMIT 10
            "#,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                - (24 * 3600)
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(error_rows
            .into_iter()
            .map(|row| {
                let error_rate =
                    (row.error_requests.unwrap_or(0) as f64 / row.total_requests as f64) * 100.0;
                (row.endpoint, error_rate)
            })
            .collect())
    }

    async fn get_throughput_trend(&self) -> Result<Vec<(SystemTime, f64)>, CoreError> {
        let trend_rows = sqlx::query!(
            r#"
            SELECT window_start, request_count
            FROM rate_limit_windows
            WHERE window_start > ? AND window_duration_seconds = 60
            ORDER BY window_start ASC
            "#,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                - (3 * 3600) // Last 3 hours
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(trend_rows
            .into_iter()
            .map(|row| {
                (
                    SystemTime::UNIX_EPOCH + Duration::from_secs(row.window_start as u64),
                    row.request_count as f64,
                )
            })
            .collect())
    }

    async fn generate_usage_trends(&self) -> Result<UsageTrends, CoreError> {
        // Get hourly counts for last 24 hours
        let hourly_counts = self.get_hourly_request_counts(24).await?;

        // Get daily counts for last 30 days
        let daily_counts = self.get_daily_request_counts(30).await?;

        // Get success rate trend
        let success_rate_trend = self.get_success_rate_trend().await?;

        // Get response time trend
        let response_time_trend = self.get_response_time_trend().await?;

        Ok(UsageTrends {
            hourly_request_counts: hourly_counts,
            daily_request_counts: daily_counts,
            success_rate_trend,
            response_time_trend,
        })
    }

    async fn get_hourly_request_counts(
        &self,
        hours: u64,
    ) -> Result<Vec<(SystemTime, u64)>, CoreError> {
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            - (hours * 3600);

        let hourly_rows = sqlx::query!(
            r#"
            SELECT 
                (timestamp / 3600) * 3600 as hour_start,
                COUNT(*) as request_count
            FROM api_call_tracking
            WHERE timestamp > ?
            GROUP BY hour_start
            ORDER BY hour_start ASC
            "#,
            cutoff
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(hourly_rows
            .into_iter()
            .map(|row| {
                (
                    SystemTime::UNIX_EPOCH + Duration::from_secs(row.hour_start as u64),
                    row.request_count as u64,
                )
            })
            .collect())
    }

    async fn get_daily_request_counts(
        &self,
        days: u64,
    ) -> Result<Vec<(SystemTime, u64)>, CoreError> {
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            - (days * 24 * 3600);

        let daily_rows = sqlx::query!(
            r#"
            SELECT 
                (timestamp / 86400) * 86400 as day_start,
                COUNT(*) as request_count
            FROM api_call_tracking
            WHERE timestamp > ?
            GROUP BY day_start
            ORDER BY day_start ASC
            "#,
            cutoff
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(daily_rows
            .into_iter()
            .map(|row| {
                (
                    SystemTime::UNIX_EPOCH + Duration::from_secs(row.day_start as u64),
                    row.request_count as u64,
                )
            })
            .collect())
    }

    async fn get_success_rate_trend(&self) -> Result<Vec<(SystemTime, f64)>, CoreError> {
        let trend_rows = sqlx::query!(
            r#"
            SELECT 
                (timestamp / 3600) * 3600 as hour_start,
                COUNT(*) as total_requests,
                SUM(CASE WHEN status_code < 400 THEN 1 ELSE 0 END) as successful_requests
            FROM api_call_tracking
            WHERE timestamp > ? AND status_code IS NOT NULL
            GROUP BY hour_start
            HAVING total_requests >= 5
            ORDER BY hour_start ASC
            "#,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                - (24 * 3600)
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(trend_rows
            .into_iter()
            .map(|row| {
                let success_rate = (row.successful_requests.unwrap_or(0) as f64
                    / row.total_requests as f64)
                    * 100.0;
                (
                    SystemTime::UNIX_EPOCH + Duration::from_secs(row.hour_start as u64),
                    success_rate,
                )
            })
            .collect())
    }

    async fn get_response_time_trend(&self) -> Result<Vec<(SystemTime, Duration)>, CoreError> {
        let trend_rows = sqlx::query!(
            r#"
            SELECT 
                (timestamp / 3600) * 3600 as hour_start,
                AVG(response_time_ms) as avg_response_time
            FROM api_call_tracking
            WHERE timestamp > ? AND status_code IS NOT NULL
            GROUP BY hour_start
            ORDER BY hour_start ASC
            "#,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                - (24 * 3600)
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(trend_rows
            .into_iter()
            .map(|row| {
                (
                    SystemTime::UNIX_EPOCH + Duration::from_secs(row.hour_start as u64),
                    Duration::from_millis(row.avg_response_time.unwrap_or(0.0) as u64),
                )
            })
            .collect())
    }

    pub async fn export_dashboard_data(&self) -> Result<String, CoreError> {
        let data = self.get_dashboard_data(false).await?;
        serde_json::to_string_pretty(&data).map_err(CoreError::Serialization)
    }
}

#[derive(Debug)]
struct CurrentWindowStats {
    request_count: i64,
    time_until_reset: Duration,
}

fn calculate_percentiles(values: &[u64]) -> (u64, u64, u64) {
    if values.is_empty() {
        return (0, 0, 0);
    }

    let len = values.len();
    let p50_idx = (len as f64 * 0.5) as usize;
    let p95_idx = (len as f64 * 0.95) as usize;
    let p99_idx = (len as f64 * 0.99) as usize;

    let p50 = values.get(p50_idx).copied().unwrap_or(0);
    let p95 = values.get(p95_idx).copied().unwrap_or(0);
    let p99 = values.get(p99_idx).copied().unwrap_or(0);

    (p50, p95, p99)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentile_calculation() {
        let values = vec![100, 200, 300, 400, 500, 600, 700, 800, 900, 1000];
        let (p50, p95, p99) = calculate_percentiles(&values);

        assert_eq!(p50, 500);
        assert_eq!(p95, 900);
        assert_eq!(p99, 1000);
    }

    #[test]
    fn test_empty_percentiles() {
        let values = vec![];
        let (p50, p95, p99) = calculate_percentiles(&values);

        assert_eq!(p50, 0);
        assert_eq!(p95, 0);
        assert_eq!(p99, 0);
    }
}
