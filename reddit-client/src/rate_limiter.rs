use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, Semaphore};
use tokio::time::sleep;

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub max_requests: u32,
    pub time_window: Duration,
    pub burst_allowance: u32,
}

impl RateLimitConfig {
    pub fn reddit_oauth() -> Self {
        Self {
            max_requests: 100, // Reddit allows 100 requests per minute for OAuth2
            time_window: Duration::from_secs(60), // 1 minute window
            burst_allowance: 10, // Allow small bursts up to 10 requests
        }
    }
}

#[derive(Debug)]
pub struct TokenBucket {
    tokens: Arc<Mutex<f64>>,
    capacity: f64,
    refill_rate: f64, // tokens per second
    last_refill: Arc<Mutex<Instant>>,
}

impl TokenBucket {
    pub fn new(config: &RateLimitConfig) -> Self {
        let capacity = config.burst_allowance as f64;
        let refill_rate = config.max_requests as f64 / config.time_window.as_secs_f64();

        Self {
            tokens: Arc::new(Mutex::new(capacity)),
            capacity,
            refill_rate,
            last_refill: Arc::new(Mutex::new(Instant::now())),
        }
    }

    pub async fn acquire(&self, tokens_needed: f64) -> Result<(), Duration> {
        let now = Instant::now();

        // Refill tokens based on elapsed time
        {
            let mut tokens = self.tokens.lock().await;
            let mut last_refill = self.last_refill.lock().await;

            let elapsed = now.duration_since(*last_refill);
            let tokens_to_add = elapsed.as_secs_f64() * self.refill_rate;

            *tokens = (*tokens + tokens_to_add).min(self.capacity);
            *last_refill = now;
        }

        // Check if we have enough tokens
        let mut tokens = self.tokens.lock().await;
        if *tokens >= tokens_needed {
            *tokens -= tokens_needed;
            Ok(())
        } else {
            // Calculate wait time for next token
            let tokens_needed_after_current = tokens_needed - *tokens;
            let wait_time = Duration::from_secs_f64(tokens_needed_after_current / self.refill_rate);
            Err(wait_time)
        }
    }

    pub async fn get_available_tokens(&self) -> f64 {
        // Update tokens first
        let now = Instant::now();
        let mut tokens = self.tokens.lock().await;
        let mut last_refill = self.last_refill.lock().await;

        let elapsed = now.duration_since(*last_refill);
        let tokens_to_add = elapsed.as_secs_f64() * self.refill_rate;

        *tokens = (*tokens + tokens_to_add).min(self.capacity);
        *last_refill = now;

        *tokens
    }
}

#[derive(Debug)]
pub struct RateLimiter {
    token_bucket: TokenBucket,
    semaphore: Arc<Semaphore>,
    config: RateLimitConfig,
    window_tracker: Arc<Mutex<WindowTracker>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.burst_allowance as usize));
        let token_bucket = TokenBucket::new(&config);
        let window_tracker = Arc::new(Mutex::new(WindowTracker::new(config.time_window)));

        Self {
            token_bucket,
            semaphore,
            config,
            window_tracker,
        }
    }

    pub async fn acquire_permit(&self) -> RateLimitPermit {
        let start_time = Instant::now();
        let _permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("Semaphore should not be closed");

        // Try to acquire token, wait if necessary
        loop {
            match self.token_bucket.acquire(1.0).await {
                Ok(()) => break,
                Err(wait_time) => {
                    tracing::debug!("Rate limit reached, waiting {:?}", wait_time);
                    sleep(wait_time).await;
                }
            }
        }

        // Track the request in our window
        {
            let mut window_tracker = self.window_tracker.lock().await;
            window_tracker.record_request();
        }

        let queue_wait_time = start_time.elapsed();
        RateLimitPermit {
            _permit,
            queue_wait_time,
        }
    }

    pub async fn get_rate_limit_status(&self) -> RateLimitStatus {
        let available_tokens = self.token_bucket.get_available_tokens().await;
        let available_permits = self.semaphore.available_permits();
        let window_tracker = self.window_tracker.lock().await;
        let window_stats = window_tracker.get_current_window_stats();

        let is_near_limit = available_tokens < (self.config.burst_allowance as f64 * 0.2);
        let estimated_wait_time = if available_tokens < 1.0 {
            Some(Duration::from_secs_f64(
                1.0 / (self.config.max_requests as f64 / 60.0),
            ))
        } else {
            None
        };

        RateLimitStatus {
            available_tokens: available_tokens as u32,
            max_tokens: self.config.burst_allowance,
            available_permits,
            max_permits: self.config.burst_allowance as usize,
            requests_per_minute: self.config.max_requests,
            current_window_requests: window_stats.request_count,
            window_start_time: window_stats.window_start,
            next_token_available_at: estimated_wait_time.map(|d| SystemTime::now() + d),
            is_near_limit,
            estimated_wait_time,
        }
    }
}

#[derive(Debug)]
pub struct RateLimitPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    pub queue_wait_time: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStatus {
    pub available_tokens: u32,
    pub max_tokens: u32,
    pub available_permits: usize,
    pub max_permits: usize,
    pub requests_per_minute: u32,
    pub current_window_requests: u32,
    pub window_start_time: SystemTime,
    pub next_token_available_at: Option<SystemTime>,
    pub is_near_limit: bool,
    pub estimated_wait_time: Option<Duration>,
}

#[derive(Debug)]
pub struct WindowTracker {
    window_duration: Duration,
    current_window: WindowStats,
}

#[derive(Debug, Clone)]
pub struct WindowStats {
    pub window_start: SystemTime,
    pub request_count: u32,
    pub successful_requests: u32,
    pub rate_limited_requests: u32,
}

impl WindowTracker {
    pub fn new(window_duration: Duration) -> Self {
        Self {
            window_duration,
            current_window: WindowStats {
                window_start: SystemTime::now(),
                request_count: 0,
                successful_requests: 0,
                rate_limited_requests: 0,
            },
        }
    }

    pub fn record_request(&mut self) {
        self.ensure_current_window();
        self.current_window.request_count += 1;
    }

    pub fn record_success(&mut self) {
        self.ensure_current_window();
        self.current_window.successful_requests += 1;
    }

    pub fn record_rate_limited(&mut self) {
        self.ensure_current_window();
        self.current_window.rate_limited_requests += 1;
    }

    pub fn get_current_window_stats(&self) -> WindowStats {
        self.current_window.clone()
    }

    fn ensure_current_window(&mut self) {
        let now = SystemTime::now();
        let window_age = now
            .duration_since(self.current_window.window_start)
            .unwrap_or_default();

        if window_age >= self.window_duration {
            // Start a new window
            self.current_window = WindowStats {
                window_start: now,
                request_count: 0,
                successful_requests: 0,
                rate_limited_requests: 0,
            };
        }
    }
}

impl RateLimitStatus {
    pub fn utilization_percentage(&self) -> f64 {
        let used_tokens = self.max_tokens - self.available_tokens;
        (used_tokens as f64 / self.max_tokens as f64) * 100.0
    }

    pub fn is_near_limit(&self) -> bool {
        self.utilization_percentage() > 80.0
    }

    pub fn requests_remaining_in_window(&self) -> u32 {
        self.requests_per_minute
            .saturating_sub(self.current_window_requests)
    }

    pub fn window_utilization_percentage(&self) -> f64 {
        (self.current_window_requests as f64 / self.requests_per_minute as f64) * 100.0
    }

    pub fn time_until_window_reset(&self) -> Duration {
        let elapsed_since_window_start = SystemTime::now()
            .duration_since(self.window_start_time)
            .unwrap_or_default();
        Duration::from_secs(60).saturating_sub(elapsed_since_window_start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_token_bucket_basic() {
        let config = RateLimitConfig {
            max_requests: 10,
            time_window: Duration::from_secs(10),
            burst_allowance: 5,
        };

        let bucket = TokenBucket::new(&config);

        // Should be able to acquire up to burst allowance
        for _ in 0..5 {
            assert!(bucket.acquire(1.0).await.is_ok());
        }

        // Next acquisition should fail
        assert!(bucket.acquire(1.0).await.is_err());
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let config = RateLimitConfig {
            max_requests: 60, // 1 token per second
            time_window: Duration::from_secs(60),
            burst_allowance: 2,
        };

        let bucket = TokenBucket::new(&config);

        // Use all tokens
        assert!(bucket.acquire(2.0).await.is_ok());
        assert!(bucket.acquire(1.0).await.is_err());

        // Wait for refill
        sleep(Duration::from_millis(1100)).await;

        // Should be able to acquire one token now
        assert!(bucket.acquire(1.0).await.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter() {
        let config = RateLimitConfig::reddit_oauth();
        let limiter = RateLimiter::new(config);

        // Should be able to acquire permits
        let _permit1 = limiter.acquire_permit().await;
        let _permit2 = limiter.acquire_permit().await;

        let status = limiter.get_rate_limit_status().await;
        assert!(status.available_tokens <= status.max_tokens);
        assert!(status.available_permits <= status.max_permits);
    }

    #[tokio::test]
    async fn test_enhanced_rate_limit_status() {
        let config = RateLimitConfig::reddit_oauth();
        let limiter = RateLimiter::new(config);

        let status = limiter.get_rate_limit_status().await;

        // Test new fields
        assert!(status.current_window_requests <= status.requests_per_minute);
        assert!(status.window_start_time <= SystemTime::now());
        assert!(!status.is_near_limit || status.available_tokens < (status.max_tokens / 5));

        // Test utilization calculation
        let utilization = status.utilization_percentage();
        assert!(utilization >= 0.0 && utilization <= 100.0);

        // Test requests remaining calculation
        let remaining = status.requests_remaining_in_window();
        assert!(remaining <= status.requests_per_minute);

        // Test window utilization
        let window_util = status.window_utilization_percentage();
        assert!(window_util >= 0.0 && window_util <= 100.0);

        // Test time until reset
        let reset_time = status.time_until_window_reset();
        assert!(reset_time <= Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_window_tracker() {
        let mut tracker = WindowTracker::new(Duration::from_secs(60));

        // Test initial state
        let stats = tracker.get_current_window_stats();
        assert_eq!(stats.request_count, 0);
        assert_eq!(stats.successful_requests, 0);
        assert_eq!(stats.rate_limited_requests, 0);

        // Test recording requests
        tracker.record_request();
        tracker.record_success();
        tracker.record_rate_limited();

        let stats = tracker.get_current_window_stats();
        assert_eq!(stats.request_count, 1);
        assert_eq!(stats.successful_requests, 1);
        assert_eq!(stats.rate_limited_requests, 1);
    }

    #[test]
    fn test_window_stats_creation() {
        let stats = WindowStats {
            window_start: SystemTime::now(),
            request_count: 5,
            successful_requests: 4,
            rate_limited_requests: 1,
        };

        assert_eq!(stats.request_count, 5);
        assert_eq!(stats.successful_requests, 4);
        assert_eq!(stats.rate_limited_requests, 1);
    }

    #[tokio::test]
    async fn test_permit_wait_time_tracking() {
        let config = RateLimitConfig::reddit_oauth();
        let limiter = RateLimiter::new(config);

        let permit = limiter.acquire_permit().await;

        // Check that queue wait time is tracked
        assert!(permit.queue_wait_time >= Duration::from_secs(0));
    }
}
