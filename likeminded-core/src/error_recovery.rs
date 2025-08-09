//! Error recovery strategies for different types of errors.
//!
//! This module provides utilities for recovering from errors in a systematic way,
//! including retry mechanisms, fallback strategies, and graceful degradation.

use crate::{CoreError, ErrorExt};
use std::time::Duration;
use tracing::info;

/// Recovery strategy for handling errors
#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    /// Retry the operation with exponential backoff
    RetryWithBackoff {
        max_attempts: usize,
        initial_delay: Duration,
        max_delay: Duration,
    },
    /// Use a fallback value or method
    Fallback,
    /// Skip the operation and continue
    Skip,
    /// Gracefully degrade functionality
    Degrade,
    /// Fail immediately
    Fail,
}

/// Result of an error recovery attempt
#[derive(Debug)]
pub enum RecoveryResult<T> {
    /// Recovery was successful, operation can continue
    Recovered(T),
    /// Recovery failed, but we can continue with degraded functionality
    Degraded(T),
    /// Recovery failed, operation should be skipped
    Skipped,
    /// Recovery failed, error should be propagated
    Failed(CoreError),
}

impl<T> RecoveryResult<T> {
    /// Returns true if the operation was successfully recovered
    pub fn is_recovered(&self) -> bool {
        matches!(self, RecoveryResult::Recovered(_))
    }

    /// Returns true if the operation can continue with degraded functionality
    pub fn is_degraded(&self) -> bool {
        matches!(self, RecoveryResult::Degraded(_))
    }

    /// Returns true if the operation should be skipped
    pub fn is_skipped(&self) -> bool {
        matches!(self, RecoveryResult::Skipped)
    }

    /// Returns true if the operation failed and error should be propagated
    pub fn is_failed(&self) -> bool {
        matches!(self, RecoveryResult::Failed(_))
    }

    /// Unwraps the value if recovered or degraded, panics otherwise
    pub fn unwrap(self) -> T {
        match self {
            RecoveryResult::Recovered(value) | RecoveryResult::Degraded(value) => value,
            _ => panic!("RecoveryResult is not recovered or degraded"),
        }
    }

    /// Returns the error if failed, None otherwise
    pub fn err(self) -> Option<CoreError> {
        match self {
            RecoveryResult::Failed(error) => Some(error),
            _ => None,
        }
    }
}

/// Error recovery handler that provides strategies for different error types
pub struct ErrorRecovery;

impl ErrorRecovery {
    /// Determine the appropriate recovery strategy for a given error
    pub fn determine_strategy(error: &CoreError) -> RecoveryStrategy {
        match error {
            // Network related errors - retry with backoff
            CoreError::Network(_)
            | CoreError::RedditApi(_)
            | CoreError::Llm(_)
            | CoreError::Embedding(_) => RecoveryStrategy::RetryWithBackoff {
                max_attempts: 3,
                initial_delay: Duration::from_secs(1),
                max_delay: Duration::from_secs(30),
            },

            // Database locked error - retry with short backoff
            CoreError::Database(db_error) => match db_error {
                crate::DatabaseError::DatabaseLocked => RecoveryStrategy::RetryWithBackoff {
                    max_attempts: 5,
                    initial_delay: Duration::from_millis(100),
                    max_delay: Duration::from_secs(5),
                },
                _ => RecoveryStrategy::Fail,
            },

            // Configuration errors - fail immediately as they need user intervention
            CoreError::Config(_) => RecoveryStrategy::Fail,

            // Timeout errors - retry once with longer timeout
            CoreError::Timeout { .. } => RecoveryStrategy::RetryWithBackoff {
                max_attempts: 1,
                initial_delay: Duration::from_secs(5),
                max_delay: Duration::from_secs(10),
            },

            // Rate limited errors - wait for specified time then retry
            CoreError::RateLimited { retry_after, .. } => {
                let delay = retry_after.unwrap_or_else(|| Duration::from_secs(60));
                RecoveryStrategy::RetryWithBackoff {
                    max_attempts: 2,
                    initial_delay: delay,
                    max_delay: Duration::from_secs(300),
                }
            }

            // Invalid input - skip as it's likely a permanent error
            CoreError::InvalidInput { .. } => RecoveryStrategy::Skip,

            // Not found errors - skip as there's nothing to recover
            CoreError::NotFound { .. } => RecoveryStrategy::Skip,

            // Permission errors - fail as they need user intervention
            CoreError::PermissionDenied { .. } => RecoveryStrategy::Fail,

            // Internal errors - try to degrade functionality
            CoreError::Internal { .. } => RecoveryStrategy::Degrade,

            // Request failed - depends on status code
            CoreError::RequestFailed { status_code, .. } => {
                match status_code {
                    Some(429) => {
                        // Rate limited
                        RecoveryStrategy::RetryWithBackoff {
                            max_attempts: 2,
                            initial_delay: Duration::from_secs(60),
                            max_delay: Duration::from_secs(300),
                        }
                    }
                    Some(500..=599) => {
                        // Server errors
                        RecoveryStrategy::RetryWithBackoff {
                            max_attempts: 3,
                            initial_delay: Duration::from_secs(5),
                            max_delay: Duration::from_secs(60),
                        }
                    }
                    _ => RecoveryStrategy::Fail,
                }
            }

            // IO and Serialization errors - retry with backoff
            CoreError::Io(_) | CoreError::Serialization(_) => RecoveryStrategy::RetryWithBackoff {
                max_attempts: 3,
                initial_delay: Duration::from_secs(1),
                max_delay: Duration::from_secs(30),
            },
        }
    }

    /// Apply the recovery strategy to an operation
    pub async fn apply_strategy<F, T, Fut>(
        strategy: RecoveryStrategy,
        mut operation: F,
    ) -> RecoveryResult<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, CoreError>> + Send,
        T: Send,
    {
        match strategy {
            RecoveryStrategy::RetryWithBackoff {
                max_attempts,
                initial_delay,
                max_delay,
            } => Self::retry_with_backoff(operation, max_attempts, initial_delay, max_delay).await,
            RecoveryStrategy::Fallback => {
                // For now, we'll treat fallback as fail since we don't have specific fallback logic
                // In a real implementation, this would try alternative approaches
                RecoveryResult::Failed(CoreError::Internal {
                    message: "Fallback strategy not implemented".to_string(),
                })
            }
            RecoveryStrategy::Skip => RecoveryResult::Skipped,
            RecoveryStrategy::Degrade => {
                // For now, we'll treat degrade as fail since we don't have specific degradation logic
                // In a real implementation, this would provide reduced functionality
                RecoveryResult::Failed(CoreError::Internal {
                    message: "Degradation strategy not implemented".to_string(),
                })
            }
            RecoveryStrategy::Fail => {
                // Execute the operation once and fail if it errors
                match operation().await {
                    Ok(value) => RecoveryResult::Recovered(value),
                    Err(error) => RecoveryResult::Failed(error),
                }
            }
        }
    }

    /// Retry an operation with exponential backoff
    async fn retry_with_backoff<F, T, Fut>(
        mut operation: F,
        max_attempts: usize,
        initial_delay: Duration,
        max_delay: Duration,
    ) -> RecoveryResult<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, CoreError>> + Send,
        T: Send,
    {
        let mut attempt = 0;
        let mut delay = initial_delay;

        loop {
            match operation().await {
                Ok(result) => return RecoveryResult::Recovered(result),
                Err(error) => {
                    attempt += 1;

                    // If we've exhausted all attempts or the error is not retryable, fail
                    if attempt >= max_attempts || !error.is_retryable() {
                        return RecoveryResult::Failed(error);
                    }

                    // Use the error's suggested retry delay if available
                    if let Some(retry_delay) = error.retry_after() {
                        delay = retry_delay;
                    }

                    // Cap the delay at the maximum
                    if delay > max_delay {
                        delay = max_delay;
                    }

                    info!(
                        "Recovery attempt {}/{} failed. Retrying after {:?}: {}",
                        attempt,
                        max_attempts,
                        delay,
                        error.user_friendly_message()
                    );

                    // Wait before retrying
                    tokio::time::sleep(delay).await;

                    // Exponential backoff (double the delay, capped at max_delay)
                    delay = std::cmp::min(delay * 2, max_delay);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DatabaseError, RedditApiError};
    use std::io;

    #[tokio::test]
    async fn test_retry_with_backoff_failure() {
        let strategy = RecoveryStrategy::RetryWithBackoff {
            max_attempts: 2,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
        };

        let result: RecoveryResult<&str> = ErrorRecovery::apply_strategy(strategy, || async {
            Err(CoreError::RedditApi(RedditApiError::RequestTimeout))
        })
        .await;

        assert!(result.is_failed());
    }

    #[tokio::test]
    async fn test_skip_strategy() {
        let strategy = RecoveryStrategy::Skip;
        let result: RecoveryResult<&str> = ErrorRecovery::apply_strategy(strategy, || async {
            Err(CoreError::InvalidInput {
                message: "test".to_string(),
            })
        })
        .await;

        assert!(result.is_skipped());
    }

    #[tokio::test]
    async fn test_determine_strategy() {
        // Test with a simple IO error since it's easier to create
        let io_error = CoreError::Io(io::Error::new(io::ErrorKind::Other, "test"));
        let strategy = ErrorRecovery::determine_strategy(&io_error);
        assert!(matches!(
            strategy,
            RecoveryStrategy::RetryWithBackoff { .. }
        ));

        let config_error = CoreError::Config(crate::ConfigError::MissingField {
            field: "test".to_string(),
        });
        let strategy = ErrorRecovery::determine_strategy(&config_error);
        assert!(matches!(strategy, RecoveryStrategy::Fail));

        let db_locked_error = CoreError::Database(DatabaseError::DatabaseLocked);
        let strategy = ErrorRecovery::determine_strategy(&db_locked_error);
        assert!(matches!(
            strategy,
            RecoveryStrategy::RetryWithBackoff {
                max_attempts: 5,
                ..
            }
        ));
    }
}
