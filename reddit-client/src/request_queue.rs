use crate::api_tracker::ApiTracker;
use likeminded_core::CoreError;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedRequest {
    pub request_id: String,
    pub endpoint: String,
    pub method: String,
    pub priority: i32,
    pub operation_type: Option<String>,
    pub access_token: String,
    pub query_params: Option<Vec<(String, String)>>,
    pub payload: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub queued_at: SystemTime,
    pub scheduled_for: Option<SystemTime>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub timeout_duration: Duration,
    pub subreddit: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RequestResult {
    pub request_id: String,
    pub success: bool,
    pub status_code: Option<u16>,
    pub response_time: Duration,
    pub error_message: Option<String>,
    pub response_data: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct PriorityRequest {
    request_id: String,
    priority: i32,
    scheduled_for: SystemTime,
}

impl Ord for PriorityRequest {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority first, then earlier scheduled time
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| self.scheduled_for.cmp(&other.scheduled_for))
    }
}

impl PartialOrd for PriorityRequest {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug)]
pub struct RequestQueue {
    pool: Arc<SqlitePool>,
    api_tracker: Option<Arc<ApiTracker>>,
    queue: Arc<RwLock<BinaryHeap<PriorityRequest>>>,
    requests: Arc<RwLock<HashMap<String, QueuedRequest>>>,
    result_senders: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<RequestResult>>>>,
    max_queue_size: usize,
    processing_enabled: bool,
}

impl RequestQueue {
    pub fn new(pool: Arc<SqlitePool>, max_queue_size: usize) -> Self {
        Self {
            pool,
            api_tracker: None,
            queue: Arc::new(RwLock::new(BinaryHeap::new())),
            requests: Arc::new(RwLock::new(HashMap::new())),
            result_senders: Arc::new(RwLock::new(HashMap::new())),
            max_queue_size,
            processing_enabled: true,
        }
    }

    pub fn with_api_tracker(mut self, api_tracker: Arc<ApiTracker>) -> Self {
        self.api_tracker = Some(api_tracker);
        self
    }

    pub async fn start_processing(&self) {
        if !self.processing_enabled {
            warn!("Request queue processing is disabled");
            return;
        }

        info!("Starting request queue processor");

        loop {
            if let Err(e) = self.process_next_request().await {
                error!("Error processing request: {}", e);
                sleep(Duration::from_millis(100)).await;
            }

            // Small delay to prevent busy waiting
            sleep(Duration::from_millis(10)).await;
        }
    }

    pub async fn enqueue_request(
        &self,
        endpoint: String,
        method: String,
        access_token: String,
        priority: i32,
        operation_type: Option<String>,
        query_params: Option<Vec<(String, String)>>,
        subreddit: Option<String>,
        timeout_duration: Option<Duration>,
    ) -> Result<(String, mpsc::UnboundedReceiver<RequestResult>), CoreError> {
        let request_id = Uuid::new_v4().to_string();
        let now = SystemTime::now();

        // Check queue size limit
        {
            let queue = self.queue.read().await;
            if queue.len() >= self.max_queue_size {
                return Err(CoreError::RateLimited {
                    message: "Request queue is full".to_string(),
                    retry_after: Some(Duration::from_secs(60)),
                });
            }
        }

        let (tx, rx) = mpsc::unbounded_channel();

        let queued_request = QueuedRequest {
            request_id: request_id.clone(),
            endpoint,
            method,
            priority,
            operation_type,
            access_token,
            query_params,
            payload: None,
            headers: None,
            queued_at: now,
            scheduled_for: Some(now),
            retry_count: 0,
            max_retries: 3,
            timeout_duration: timeout_duration.unwrap_or(Duration::from_secs(30)),
            subreddit,
        };

        // Save request to database
        self.save_queued_request(&queued_request).await?;

        // Add to in-memory structures
        {
            let mut queue = self.queue.write().await;
            let mut requests = self.requests.write().await;
            let mut senders = self.result_senders.write().await;

            queue.push(PriorityRequest {
                request_id: request_id.clone(),
                priority,
                scheduled_for: now,
            });

            requests.insert(request_id.clone(), queued_request.clone());
            senders.insert(request_id.clone(), tx);
        }

        debug!(
            "Enqueued request {} with priority {} for endpoint {}",
            request_id, priority, queued_request.endpoint
        );

        Ok((request_id, rx))
    }

    async fn process_next_request(&self) -> Result<(), CoreError> {
        let next_request = {
            let mut queue = self.queue.write().await;
            queue.pop()
        };

        if let Some(priority_req) = next_request {
            // Check if request is scheduled for the future
            if priority_req.scheduled_for > SystemTime::now() {
                // Put it back and wait
                {
                    let mut queue = self.queue.write().await;
                    queue.push(priority_req);
                }
                sleep(Duration::from_millis(100)).await;
                return Ok(());
            }

            let request = {
                let requests = self.requests.read().await;
                requests.get(&priority_req.request_id).cloned()
            };

            if let Some(mut request) = request {
                self.execute_request(&mut request).await?;
            }
        } else {
            // No requests in queue, wait a bit
            sleep(Duration::from_millis(100)).await;
        }

        Ok(())
    }

    async fn execute_request(&self, request: &mut QueuedRequest) -> Result<(), CoreError> {
        debug!(
            "Executing request {} for {}",
            request.request_id, request.endpoint
        );

        // Update request status to executing
        self.update_request_status(&request.request_id, "executing")
            .await?;

        let start_time = SystemTime::now();
        let mut result = RequestResult {
            request_id: request.request_id.clone(),
            success: false,
            status_code: None,
            response_time: Duration::from_secs(0),
            error_message: None,
            response_data: None,
        };

        // Simulate API request execution
        // In a real implementation, this would call the actual API client
        match self.simulate_api_request(request).await {
            Ok((status_code, response_data)) => {
                result.success = status_code < 400;
                result.status_code = Some(status_code);
                result.response_data = Some(response_data);
                result.response_time = start_time.elapsed().unwrap_or_default();

                if result.success {
                    self.complete_request(request, &result).await?;
                } else {
                    self.handle_request_failure(request, &result).await?;
                }
            }
            Err(e) => {
                result.error_message = Some(e.to_string());
                result.response_time = start_time.elapsed().unwrap_or_default();

                self.handle_request_failure(request, &result).await?;
            }
        }

        // Send result to waiting caller
        {
            let senders = self.result_senders.read().await;
            if let Some(sender) = senders.get(&request.request_id) {
                if let Err(_) = sender.send(result) {
                    warn!("Failed to send result for request {}", request.request_id);
                }
            }
        }

        Ok(())
    }

    async fn simulate_api_request(
        &self,
        request: &QueuedRequest,
    ) -> Result<(u16, String), CoreError> {
        // This is a placeholder - in real implementation, this would use the actual API client
        // For now, we'll simulate different responses based on endpoint patterns

        sleep(Duration::from_millis(50 + (request.priority * 10) as u64)).await;

        let status_code = if request.endpoint.contains("nonexistent") {
            404
        } else if request.retry_count > 0 {
            200 // Succeed on retry
        } else if request.priority < 0 {
            // Low priority requests might get rate limited more often
            if rand::random::<f32>() < 0.3 {
                429
            } else {
                200
            }
        } else {
            200
        };

        let response_data = format!(
            "{{\"endpoint\": \"{}\", \"method\": \"{}\", \"status\": {}}}",
            request.endpoint, request.method, status_code
        );

        if status_code == 429 {
            Err(CoreError::RateLimited {
                message: "Rate limited".to_string(),
                retry_after: Some(Duration::from_secs(60)),
            })
        } else if status_code >= 500 {
            Err(CoreError::RequestFailed {
                message: "Server error".to_string(),
                status_code: Some(status_code),
            })
        } else {
            Ok((status_code, response_data))
        }
    }

    async fn complete_request(
        &self,
        request: &QueuedRequest,
        result: &RequestResult,
    ) -> Result<(), CoreError> {
        // Update database
        self.update_request_status(&request.request_id, "completed")
            .await?;

        // Remove from in-memory structures
        {
            let mut requests = self.requests.write().await;
            let mut senders = self.result_senders.write().await;

            requests.remove(&request.request_id);
            senders.remove(&request.request_id);
        }

        // Record metrics if tracker is available
        if let Some(ref tracker) = self.api_tracker {
            let _ = tracker
                .record_api_call(
                    &request.endpoint,
                    &request.method,
                    result.status_code,
                    result.response_time,
                    false, // Not rate limited if we completed successfully
                    request.priority,
                    Duration::from_secs(0), // No additional queue wait time
                    request.operation_type.as_deref(),
                    request.subreddit.as_deref(),
                    None,
                    None,
                )
                .await;
        }

        debug!("Completed request {} successfully", request.request_id);
        Ok(())
    }

    async fn handle_request_failure(
        &self,
        request: &mut QueuedRequest,
        _result: &RequestResult,
    ) -> Result<(), CoreError> {
        request.retry_count += 1;

        if request.retry_count <= request.max_retries {
            // Schedule for retry with exponential backoff
            let backoff_seconds = 2_u64.pow(request.retry_count) * 60; // 2, 4, 8 minutes
            let retry_time = SystemTime::now() + Duration::from_secs(backoff_seconds);

            request.scheduled_for = Some(retry_time);

            // Put back in queue
            {
                let mut queue = self.queue.write().await;
                let mut requests = self.requests.write().await;

                queue.push(PriorityRequest {
                    request_id: request.request_id.clone(),
                    priority: request.priority,
                    scheduled_for: retry_time,
                });

                requests.insert(request.request_id.clone(), request.clone());
            }

            self.update_request_retry_info(&request.request_id, request.retry_count, retry_time)
                .await?;

            warn!(
                "Request {} failed, scheduling retry {} in {} seconds",
                request.request_id, request.retry_count, backoff_seconds
            );
        } else {
            // Max retries exceeded, mark as failed
            self.update_request_status(&request.request_id, "failed")
                .await?;

            // Remove from in-memory structures
            {
                let mut requests = self.requests.write().await;
                let mut senders = self.result_senders.write().await;

                requests.remove(&request.request_id);
                senders.remove(&request.request_id);
            }

            error!(
                "Request {} failed permanently after {} retries",
                request.request_id, request.retry_count
            );
        }

        Ok(())
    }

    async fn save_queued_request(&self, request: &QueuedRequest) -> Result<(), CoreError> {
        let scheduled_timestamp = request
            .scheduled_for
            .unwrap_or(request.queued_at)
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let queued_timestamp = request
            .queued_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        sqlx::query!(
            r#"
            INSERT INTO request_queue (
                request_id, endpoint, method, priority, operation_type,
                queued_at, scheduled_for, status, retry_count, max_retries
            ) VALUES (?, ?, ?, ?, ?, ?, ?, 'queued', ?, ?)
            "#,
            request.request_id,
            request.endpoint,
            request.method,
            request.priority,
            request.operation_type,
            queued_timestamp,
            scheduled_timestamp,
            request.retry_count,
            request.max_retries
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(())
    }

    async fn update_request_status(&self, request_id: &str, status: &str) -> Result<(), CoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let field = match status {
            "executing" => "started_at",
            "completed" => "completed_at",
            "failed" => "failed_at",
            _ => return Ok(()),
        };

        sqlx::query(&format!(
            "UPDATE request_queue SET status = ?, {} = ? WHERE request_id = ?",
            field
        ))
        .bind(status)
        .bind(now)
        .bind(request_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(())
    }

    async fn update_request_retry_info(
        &self,
        request_id: &str,
        retry_count: u32,
        scheduled_for: SystemTime,
    ) -> Result<(), CoreError> {
        let scheduled_timestamp = scheduled_for
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        sqlx::query!(
            "UPDATE request_queue SET retry_count = ?, scheduled_for = ? WHERE request_id = ?",
            retry_count,
            scheduled_timestamp,
            request_id
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        Ok(())
    }

    pub async fn get_queue_stats(&self) -> Result<QueueStats, CoreError> {
        let queue_size = {
            let queue = self.queue.read().await;
            queue.len()
        };

        let requests_by_status = sqlx::query!(
            r#"
            SELECT status, COUNT(*) as count
            FROM request_queue
            WHERE completed_at IS NULL AND failed_at IS NULL
            GROUP BY status
            "#
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| CoreError::Database(likeminded_core::DatabaseError::Sql(e)))?;

        let mut stats = QueueStats::default();
        stats.total_queued = queue_size;

        for row in requests_by_status {
            match row.status.as_str() {
                "queued" => stats.queued = row.count as usize,
                "executing" => stats.executing = row.count as usize,
                _ => {}
            }
        }

        Ok(stats)
    }

    pub async fn cancel_request(&self, request_id: &str) -> Result<bool, CoreError> {
        // Remove from queue and requests map
        let found = {
            let mut queue = self.queue.write().await;
            let mut requests = self.requests.write().await;
            let mut senders = self.result_senders.write().await;

            // Remove from priority queue (this is O(n) but queue should be small)
            let mut temp_vec: Vec<PriorityRequest> = queue.drain().collect();
            temp_vec.retain(|req| req.request_id != request_id);
            for req in temp_vec {
                queue.push(req);
            }

            let removed = requests.remove(request_id).is_some();
            senders.remove(request_id);
            removed
        };

        if found {
            // Update database
            self.update_request_status(request_id, "cancelled").await?;
            debug!("Cancelled request {}", request_id);
        }

        Ok(found)
    }
}

#[derive(Debug, Default, Clone)]
pub struct QueueStats {
    pub total_queued: usize,
    pub queued: usize,
    pub executing: usize,
    pub completed_today: u64,
    pub failed_today: u64,
    pub average_wait_time: Duration,
}

// Add rand dependency for simulation
#[cfg(not(target_arch = "wasm32"))]
mod rand {
    pub fn random<T>() -> T
    where
        T: From<f32>,
    {
        // Simple pseudo-random for simulation
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::time::SystemTime;

        let mut hasher = DefaultHasher::new();
        SystemTime::now().hash(&mut hasher);
        let hash = hasher.finish();
        T::from((hash % 1000) as f32 / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_request_ordering() {
        let mut heap = BinaryHeap::new();

        heap.push(PriorityRequest {
            request_id: "low".to_string(),
            priority: -1,
            scheduled_for: SystemTime::now(),
        });

        heap.push(PriorityRequest {
            request_id: "high".to_string(),
            priority: 1,
            scheduled_for: SystemTime::now(),
        });

        heap.push(PriorityRequest {
            request_id: "normal".to_string(),
            priority: 0,
            scheduled_for: SystemTime::now(),
        });

        assert_eq!(heap.pop().unwrap().request_id, "high");
        assert_eq!(heap.pop().unwrap().request_id, "normal");
        assert_eq!(heap.pop().unwrap().request_id, "low");
    }
}
