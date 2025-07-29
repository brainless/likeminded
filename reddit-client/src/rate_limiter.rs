use std::sync::Arc;
use std::time::{Duration, Instant};
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
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.burst_allowance as usize));
        let token_bucket = TokenBucket::new(&config);

        Self {
            token_bucket,
            semaphore,
            config,
        }
    }

    pub async fn acquire_permit(&self) -> RateLimitPermit {
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

        RateLimitPermit { _permit }
    }

    pub async fn get_rate_limit_status(&self) -> RateLimitStatus {
        let available_tokens = self.token_bucket.get_available_tokens().await;
        let available_permits = self.semaphore.available_permits();

        RateLimitStatus {
            available_tokens: available_tokens as u32,
            max_tokens: self.config.burst_allowance,
            available_permits,
            max_permits: self.config.burst_allowance as usize,
            requests_per_minute: self.config.max_requests,
        }
    }
}

#[derive(Debug)]
pub struct RateLimitPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

#[derive(Debug, Clone)]
pub struct RateLimitStatus {
    pub available_tokens: u32,
    pub max_tokens: u32,
    pub available_permits: usize,
    pub max_permits: usize,
    pub requests_per_minute: u32,
}

impl RateLimitStatus {
    pub fn utilization_percentage(&self) -> f64 {
        let used_tokens = self.max_tokens - self.available_tokens;
        (used_tokens as f64 / self.max_tokens as f64) * 100.0
    }

    pub fn is_near_limit(&self) -> bool {
        self.utilization_percentage() > 80.0
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
}
