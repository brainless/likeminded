use likeminded_core::{CoreError, RedditApiError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Base delay for exponential backoff (in milliseconds)
    pub base_delay_ms: u64,
    /// Maximum delay between retries (in milliseconds)
    pub max_delay_ms: u64,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Maximum jitter factor (0.0 to 1.0)
    pub jitter_factor: f64,
    /// Circuit breaker failure threshold
    pub failure_threshold: u32,
    /// Circuit breaker recovery timeout (in seconds)
    pub recovery_timeout_s: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 1000, // 1 second
            max_delay_ms: 30000, // 30 seconds
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,     // 10% jitter
            failure_threshold: 5,   // Circuit breaker after 5 consecutive failures
            recovery_timeout_s: 60, // Try recovery after 1 minute
        }
    }
}

impl RetryConfig {
    /// Create retry config optimized for Reddit API
    pub fn reddit() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 2000, // Start with 2 seconds for Reddit API
            max_delay_ms: 60000, // Max 1 minute delay
            backoff_multiplier: 2.0,
            jitter_factor: 0.2,      // 20% jitter to prevent thundering herd
            failure_threshold: 3,    // More aggressive circuit breaking for API
            recovery_timeout_s: 120, // 2 minute recovery window
        }
    }
}

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerState {
    Closed,   // Normal operation
    Open,     // Blocking requests
    HalfOpen, // Testing recovery
}

/// Circuit breaker for preventing cascading failures
#[derive(Debug)]
pub struct CircuitBreaker {
    state: CircuitBreakerState,
    failure_count: u32,
    last_failure_time: Option<Instant>,
    config: RetryConfig,
}

impl CircuitBreaker {
    pub fn new(config: RetryConfig) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            last_failure_time: None,
            config,
        }
    }

    /// Check if a request should be allowed
    pub fn allow_request(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    let recovery_duration = Duration::from_secs(self.config.recovery_timeout_s);
                    if last_failure.elapsed() >= recovery_duration {
                        debug!("Circuit breaker transitioning to half-open for recovery test");
                        self.state = CircuitBreakerState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }

    /// Record a successful request
    pub fn record_success(&mut self) {
        match self.state {
            CircuitBreakerState::HalfOpen => {
                info!("Circuit breaker recovery successful, returning to closed state");
                self.state = CircuitBreakerState::Closed;
                self.failure_count = 0;
                self.last_failure_time = None;
            }
            _ => {
                // Reset failure count on success
                self.failure_count = 0;
            }
        }
    }

    /// Record a failed request
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(Instant::now());

        match self.state {
            CircuitBreakerState::Closed => {
                if self.failure_count >= self.config.failure_threshold {
                    warn!(
                        "Circuit breaker opening due to {} consecutive failures",
                        self.failure_count
                    );
                    self.state = CircuitBreakerState::Open;
                }
            }
            CircuitBreakerState::HalfOpen => {
                warn!("Circuit breaker recovery failed, returning to open state");
                self.state = CircuitBreakerState::Open;
            }
            CircuitBreakerState::Open => {
                // Already open, just update failure time
            }
        }
    }

    pub fn get_state(&self) -> CircuitBreakerState {
        self.state.clone()
    }
}

/// Retry strategy based on error type
#[derive(Debug, Clone, PartialEq)]
pub enum RetryStrategy {
    /// Retry with exponential backoff
    Retry,
    /// Retry immediately (for rate limits with specific retry-after)
    RetryWithDelay(Duration),
    /// Don't retry (for permanent failures)
    NoRetry,
}

/// Determine retry strategy based on error type
pub fn get_retry_strategy(error: &CoreError) -> RetryStrategy {
    match error {
        CoreError::RedditApi(reddit_error) => match reddit_error {
            // Rate limits should be retried with specific delay
            RedditApiError::RateLimitExceeded { retry_after } => {
                RetryStrategy::RetryWithDelay(Duration::from_secs(*retry_after))
            }
            // Server errors are usually transient
            RedditApiError::ServerError { .. } => RetryStrategy::Retry,
            // Request timeouts should be retried
            RedditApiError::RequestTimeout => RetryStrategy::Retry,
            // Some responses might be transient
            RedditApiError::InvalidResponse { .. } => RetryStrategy::Retry,
            // Authentication and permission errors are permanent
            RedditApiError::AuthenticationFailed { .. } => RetryStrategy::NoRetry,
            RedditApiError::InvalidToken => RetryStrategy::NoRetry,
            RedditApiError::Forbidden { .. } => RetryStrategy::NoRetry,
            // Not found errors are permanent
            RedditApiError::SubredditNotFound { .. } => RetryStrategy::NoRetry,
            RedditApiError::PostNotFound { .. } => RetryStrategy::NoRetry,
            RedditApiError::EndpointUnavailable { .. } => RetryStrategy::Retry,
        },
        // Network errors might be transient
        CoreError::Network(reqwest_error) => {
            if reqwest_error.is_timeout() || reqwest_error.is_connect() {
                RetryStrategy::Retry
            } else {
                RetryStrategy::NoRetry
            }
        }
        // Other errors are usually not worth retrying
        _ => RetryStrategy::NoRetry,
    }
}

/// Calculate delay with exponential backoff and jitter
pub fn calculate_delay(attempt: u32, config: &RetryConfig) -> Duration {
    let base_delay = Duration::from_millis(config.base_delay_ms);
    let max_delay = Duration::from_millis(config.max_delay_ms);

    // Calculate exponential backoff
    let exponential_delay = if attempt == 0 {
        base_delay
    } else {
        let multiplier = config.backoff_multiplier.powi(attempt as i32);
        let delay_ms = (config.base_delay_ms as f64 * multiplier) as u64;
        Duration::from_millis(delay_ms.min(config.max_delay_ms))
    };

    // Add jitter to prevent thundering herd
    let jitter_range = (exponential_delay.as_millis() as f64 * config.jitter_factor) as u64;
    let jitter = fastrand::u64(0..=jitter_range);
    let final_delay = exponential_delay + Duration::from_millis(jitter);

    // Ensure we don't exceed max delay
    final_delay.min(max_delay)
}

/// Retry metrics for monitoring
#[derive(Debug, Clone)]
pub struct RetryMetrics {
    pub total_retries: u64,
    pub successful_retries: u64,
    pub failed_retries: u64,
    pub circuit_breaker_trips: u64,
    pub average_retry_delay_ms: f64,
}

impl Default for RetryMetrics {
    fn default() -> Self {
        Self {
            total_retries: 0,
            successful_retries: 0,
            failed_retries: 0,
            circuit_breaker_trips: 0,
            average_retry_delay_ms: 0.0,
        }
    }
}

/// Retry executor that wraps operations with retry logic
#[derive(Debug)]
pub struct RetryExecutor {
    config: RetryConfig,
    circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    metrics: Arc<Mutex<RetryMetrics>>,
}

impl RetryExecutor {
    pub fn new(config: RetryConfig) -> Self {
        let circuit_breaker = Arc::new(Mutex::new(CircuitBreaker::new(config.clone())));
        let metrics = Arc::new(Mutex::new(RetryMetrics::default()));

        Self {
            config,
            circuit_breaker,
            metrics,
        }
    }

    /// Execute an operation with retry logic
    pub async fn execute<F, Fut, T>(
        &self,
        operation_name: &str,
        operation: F,
    ) -> Result<T, CoreError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, CoreError>>,
    {
        // Check circuit breaker first
        {
            let mut breaker = self.circuit_breaker.lock().unwrap();
            if !breaker.allow_request() {
                let mut metrics = self.metrics.lock().unwrap();
                metrics.circuit_breaker_trips += 1;
                drop(metrics);
                drop(breaker);

                warn!(
                    "Circuit breaker is open, blocking request for {}",
                    operation_name
                );
                return Err(CoreError::Internal {
                    message: "Circuit breaker is open".to_string(),
                });
            }
        }

        let mut last_error: Option<String> = None;
        let mut total_delay_ms = 0u64;

        for attempt in 0..self.config.max_attempts {
            if attempt > 0 {
                debug!("Retry attempt {} for {}", attempt, operation_name);
            }

            let start_time = Instant::now();
            match operation().await {
                Ok(result) => {
                    // Success - record in circuit breaker and metrics
                    {
                        let mut breaker = self.circuit_breaker.lock().unwrap();
                        breaker.record_success();
                    }

                    if attempt > 0 {
                        let mut metrics = self.metrics.lock().unwrap();
                        metrics.total_retries += attempt as u64;
                        metrics.successful_retries += 1;
                        metrics.average_retry_delay_ms = (metrics.average_retry_delay_ms
                            * (metrics.successful_retries - 1) as f64
                            + total_delay_ms as f64)
                            / metrics.successful_retries as f64;

                        info!(
                            "Operation {} succeeded after {} retries (total delay: {}ms)",
                            operation_name, attempt, total_delay_ms
                        );
                    }

                    return Ok(result);
                }
                Err(error) => {
                    let elapsed = start_time.elapsed();

                    debug!(
                        "Attempt {} failed for {} after {:?}: {}",
                        attempt + 1,
                        operation_name,
                        elapsed,
                        error
                    );

                    // Determine if we should retry
                    let strategy = get_retry_strategy(&error);
                    let should_retry = attempt + 1 < self.config.max_attempts;

                    match strategy {
                        RetryStrategy::NoRetry => {
                            debug!(
                                "Not retrying {} due to error type: {}",
                                operation_name, error
                            );
                            last_error = Some(error.to_string());
                            break;
                        }
                        RetryStrategy::Retry if should_retry => {
                            let delay = calculate_delay(attempt, &self.config);
                            total_delay_ms += delay.as_millis() as u64;

                            info!(
                                "Retrying {} in {:?} due to: {}",
                                operation_name, delay, error
                            );

                            last_error = Some(error.to_string());
                            sleep(delay).await;
                        }
                        RetryStrategy::RetryWithDelay(delay) if should_retry => {
                            total_delay_ms += delay.as_millis() as u64;

                            info!(
                                "Retrying {} after specified delay of {:?} due to: {}",
                                operation_name, delay, error
                            );

                            last_error = Some(error.to_string());
                            sleep(delay).await;
                        }
                        _ => {
                            // Max attempts reached or no retry strategy
                            if should_retry {
                                debug!("Max retry attempts reached for {}", operation_name);
                            }
                            last_error = Some(error.to_string());
                            break;
                        }
                    }
                }
            }
        }

        // All retries failed - record failure in circuit breaker and metrics
        {
            let mut breaker = self.circuit_breaker.lock().unwrap();
            breaker.record_failure();
        }

        {
            let mut metrics = self.metrics.lock().unwrap();
            metrics.failed_retries += 1;
        }

        error!(
            "Operation {} failed after {} attempts with total delay of {}ms",
            operation_name, self.config.max_attempts, total_delay_ms
        );

        Err(CoreError::Internal {
            message: last_error
                .unwrap_or_else(|| "Unknown error during retry execution".to_string()),
        })
    }

    /// Get current retry metrics
    pub fn get_metrics(&self) -> RetryMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Get current circuit breaker state
    pub fn get_circuit_breaker_state(&self) -> CircuitBreakerState {
        self.circuit_breaker.lock().unwrap().get_state()
    }

    /// Reset metrics (useful for testing or periodic cleanup)
    pub fn reset_metrics(&self) {
        let mut metrics = self.metrics.lock().unwrap();
        *metrics = RetryMetrics::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.base_delay_ms, 1000);
        assert!(config.jitter_factor <= 1.0);
    }

    #[test]
    fn test_retry_config_reddit() {
        let config = RetryConfig::reddit();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.base_delay_ms, 2000);
        assert_eq!(config.jitter_factor, 0.2);
    }

    #[test]
    fn test_circuit_breaker_closed_state() {
        let config = RetryConfig::default();
        let mut breaker = CircuitBreaker::new(config);

        assert_eq!(breaker.get_state(), CircuitBreakerState::Closed);
        assert!(breaker.allow_request());
    }

    #[test]
    fn test_circuit_breaker_failure_threshold() {
        let mut config = RetryConfig::default();
        config.failure_threshold = 2;
        let mut breaker = CircuitBreaker::new(config);

        // First failure - should remain closed
        breaker.record_failure();
        assert_eq!(breaker.get_state(), CircuitBreakerState::Closed);
        assert!(breaker.allow_request());

        // Second failure - should open
        breaker.record_failure();
        assert_eq!(breaker.get_state(), CircuitBreakerState::Open);
        assert!(!breaker.allow_request());
    }

    #[test]
    fn test_circuit_breaker_recovery() {
        let mut config = RetryConfig::default();
        config.failure_threshold = 1;
        config.recovery_timeout_s = 0; // Immediate recovery for test
        let mut breaker = CircuitBreaker::new(config);

        // Trip the breaker
        breaker.record_failure();
        assert_eq!(breaker.get_state(), CircuitBreakerState::Open);

        // Should allow recovery attempt after timeout
        std::thread::sleep(Duration::from_millis(1));
        assert!(breaker.allow_request());
        assert_eq!(breaker.get_state(), CircuitBreakerState::HalfOpen);

        // Successful recovery
        breaker.record_success();
        assert_eq!(breaker.get_state(), CircuitBreakerState::Closed);
    }

    #[test]
    fn test_retry_strategy_for_errors() {
        let rate_limit_error =
            CoreError::RedditApi(RedditApiError::RateLimitExceeded { retry_after: 60 });
        match get_retry_strategy(&rate_limit_error) {
            RetryStrategy::RetryWithDelay(delay) => {
                assert_eq!(delay, Duration::from_secs(60));
            }
            _ => panic!("Expected RetryWithDelay for rate limit error"),
        }

        let auth_error = CoreError::RedditApi(RedditApiError::AuthenticationFailed {
            reason: "Invalid token".to_string(),
        });
        assert_eq!(get_retry_strategy(&auth_error), RetryStrategy::NoRetry);

        let server_error = CoreError::RedditApi(RedditApiError::ServerError { status_code: 500 });
        assert_eq!(get_retry_strategy(&server_error), RetryStrategy::Retry);
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            max_delay_ms: 10000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0, // No jitter for predictable test
            ..Default::default()
        };

        let delay_0 = calculate_delay(0, &config);
        assert_eq!(delay_0, Duration::from_millis(1000));

        let delay_1 = calculate_delay(1, &config);
        assert_eq!(delay_1, Duration::from_millis(2000));

        let delay_2 = calculate_delay(2, &config);
        assert_eq!(delay_2, Duration::from_millis(4000));

        let delay_3 = calculate_delay(3, &config);
        assert_eq!(delay_3, Duration::from_millis(8000));

        // Should cap at max_delay_ms
        let delay_10 = calculate_delay(10, &config);
        assert_eq!(delay_10, Duration::from_millis(10000));
    }

    #[test]
    fn test_jitter_adds_randomness() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            max_delay_ms: 10000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.5, // 50% jitter
            ..Default::default()
        };

        let delay_1 = calculate_delay(1, &config);
        let delay_2 = calculate_delay(1, &config);

        // With jitter, delays should potentially be different
        // (Though they might occasionally be the same due to randomness)
        // At minimum, they should be within the expected range
        assert!(delay_1 >= Duration::from_millis(2000));
        assert!(delay_1 <= Duration::from_millis(3000)); // base 2000 + 50% jitter
        assert!(delay_2 >= Duration::from_millis(2000));
        assert!(delay_2 <= Duration::from_millis(3000));
    }

    #[tokio::test]
    async fn test_retry_executor_success_on_first_attempt() {
        let config = RetryConfig {
            max_attempts: 3,
            ..Default::default()
        };
        let executor = RetryExecutor::new(config);

        let result = executor
            .execute("test_operation", || async { Ok::<i32, CoreError>(42) })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);

        let metrics = executor.get_metrics();
        assert_eq!(metrics.total_retries, 0);
        assert_eq!(metrics.successful_retries, 0); // No retries needed
    }

    #[tokio::test]
    async fn test_retry_executor_success_after_retries() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay_ms: 1, // Very short delay for test
            ..Default::default()
        };
        let executor = RetryExecutor::new(config);

        let attempt_count = Arc::new(std::sync::Mutex::new(0));
        let attempt_count_clone = attempt_count.clone();

        let result = executor
            .execute("test_operation", move || {
                let attempt_count = attempt_count_clone.clone();
                async move {
                    let mut count = attempt_count.lock().unwrap();
                    *count += 1;
                    if *count < 3 {
                        // Fail first two attempts
                        Err(CoreError::RedditApi(RedditApiError::ServerError {
                            status_code: 500,
                        }))
                    } else {
                        // Succeed on third attempt
                        Ok(42)
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);

        let metrics = executor.get_metrics();
        assert_eq!(metrics.total_retries, 2); // 2 retries before success
        assert_eq!(metrics.successful_retries, 1);
    }

    #[tokio::test]
    async fn test_retry_executor_no_retry_on_auth_error() {
        let config = RetryConfig {
            max_attempts: 3,
            ..Default::default()
        };
        let executor = RetryExecutor::new(config);

        let attempt_count = Arc::new(std::sync::Mutex::new(0));
        let attempt_count_clone = attempt_count.clone();

        let result = executor
            .execute("test_operation", move || {
                let attempt_count = attempt_count_clone.clone();
                async move {
                    let mut count = attempt_count.lock().unwrap();
                    *count += 1;
                    Err::<i32, CoreError>(CoreError::RedditApi(
                        RedditApiError::AuthenticationFailed {
                            reason: "Invalid token".to_string(),
                        },
                    ))
                }
            })
            .await;

        assert!(result.is_err());

        // Should only attempt once (no retries for auth errors)
        let count = attempt_count.lock().unwrap();
        assert_eq!(*count, 1);

        let metrics = executor.get_metrics();
        assert_eq!(metrics.total_retries, 0); // No retries attempted
        assert_eq!(metrics.failed_retries, 1);
    }

    #[tokio::test]
    async fn test_retry_executor_circuit_breaker() {
        let config = RetryConfig {
            max_attempts: 2,
            failure_threshold: 2, // Trip after 2 failures
            base_delay_ms: 1,
            ..Default::default()
        };
        let executor = RetryExecutor::new(config);

        // First operation fails completely
        let result1 = executor
            .execute("test_operation_1", || async {
                Err::<i32, CoreError>(CoreError::RedditApi(RedditApiError::ServerError {
                    status_code: 500,
                }))
            })
            .await;
        assert!(result1.is_err());

        // Second operation fails completely - should trip circuit breaker
        let result2 = executor
            .execute("test_operation_2", || async {
                Err::<i32, CoreError>(CoreError::RedditApi(RedditApiError::ServerError {
                    status_code: 500,
                }))
            })
            .await;
        assert!(result2.is_err());

        // Circuit breaker should now be open
        assert_eq!(
            executor.get_circuit_breaker_state(),
            CircuitBreakerState::Open
        );

        // Third operation should be blocked by circuit breaker
        let result3 = executor
            .execute("test_operation_3", || async {
                Ok::<i32, CoreError>(42) // This would succeed, but circuit breaker blocks it
            })
            .await;
        assert!(result3.is_err());
        assert!(result3
            .unwrap_err()
            .to_string()
            .contains("Circuit breaker is open"));

        let metrics = executor.get_metrics();
        assert_eq!(metrics.circuit_breaker_trips, 1);
    }
}
