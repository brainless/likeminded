use crate::error::*;
use std::time::Duration;
use tracing::{error, info, warn};

pub trait ErrorExt {
    fn log_error(&self) -> &Self;
    fn log_warn(&self) -> &Self;
    fn is_retryable(&self) -> bool;
    fn retry_after(&self) -> Option<Duration>;
    fn user_friendly_message(&self) -> String;
    fn error_code(&self) -> String;
}

impl ErrorExt for CoreError {
    fn log_error(&self) -> &Self {
        error!("CoreError: {}", self);
        match self {
            CoreError::RedditApi(e) => {
                error!("Reddit API error details: {:?}", e);
            }
            CoreError::Database(e) => {
                error!("Database error details: {:?}", e);
            }
            CoreError::Llm(e) => {
                error!("LLM error details: {:?}", e);
            }
            CoreError::Embedding(e) => {
                error!("Embedding error details: {:?}", e);
            }
            CoreError::Config(e) => {
                error!("Configuration error details: {:?}", e);
            }
            _ => {}
        }
        self
    }

    fn log_warn(&self) -> &Self {
        warn!("CoreError (warning): {}", self);
        self
    }

    fn is_retryable(&self) -> bool {
        match self {
            CoreError::RedditApi(e) => e.is_retryable(),
            CoreError::Database(e) => e.is_retryable(),
            CoreError::Llm(e) => e.is_retryable(),
            CoreError::Embedding(e) => e.is_retryable(),
            CoreError::Network(_) => true,
            CoreError::Timeout { .. } => true,
            CoreError::RateLimited { .. } => true,
            CoreError::RequestFailed { .. } => false,
            _ => false,
        }
    }

    fn retry_after(&self) -> Option<Duration> {
        match self {
            CoreError::RedditApi(RedditApiError::RateLimitExceeded { retry_after }) => {
                Some(Duration::from_secs(*retry_after))
            }
            CoreError::Llm(LlmError::RateLimitExceeded { retry_after, .. }) => {
                Some(Duration::from_secs(*retry_after))
            }
            CoreError::Timeout { seconds } => Some(Duration::from_secs(*seconds)),
            CoreError::RateLimited { retry_after, .. } => *retry_after,
            _ if self.is_retryable() => Some(Duration::from_secs(5)), // Default retry delay
            _ => None,
        }
    }

    fn user_friendly_message(&self) -> String {
        match self {
            CoreError::RedditApi(e) => e.user_friendly_message(),
            CoreError::Database(e) => e.user_friendly_message(),
            CoreError::Llm(e) => e.user_friendly_message(),
            CoreError::Embedding(e) => e.user_friendly_message(),
            CoreError::Config(e) => e.user_friendly_message(),
            CoreError::Network(_) => {
                "Network connection error. Please check your internet connection.".to_string()
            }
            CoreError::InvalidInput { .. } => {
                "Invalid input provided. Please check your input and try again.".to_string()
            }
            CoreError::Timeout { .. } => {
                "The operation took too long to complete. Please try again.".to_string()
            }
            CoreError::NotFound { resource } => format!("Could not find: {}", resource),
            CoreError::PermissionDenied { operation } => {
                format!("Permission denied for: {}", operation)
            }
            CoreError::RateLimited { message, .. } => {
                format!(
                    "Rate limited: {}. Please wait before trying again.",
                    message
                )
            }
            CoreError::RequestFailed { message, .. } => {
                format!("Request failed: {}", message)
            }
            _ => "An unexpected error occurred. Please try again later.".to_string(),
        }
    }

    fn error_code(&self) -> String {
        match self {
            CoreError::RedditApi(_) => "REDDIT_API".to_string(),
            CoreError::Database(_) => "DATABASE".to_string(),
            CoreError::Llm(_) => "LLM".to_string(),
            CoreError::Embedding(_) => "EMBEDDING".to_string(),
            CoreError::Config(_) => "CONFIG".to_string(),
            CoreError::Io(_) => "IO".to_string(),
            CoreError::Serialization(_) => "SERIALIZATION".to_string(),
            CoreError::Network(_) => "NETWORK".to_string(),
            CoreError::InvalidInput { .. } => "INVALID_INPUT".to_string(),
            CoreError::Timeout { .. } => "TIMEOUT".to_string(),
            CoreError::NotFound { .. } => "NOT_FOUND".to_string(),
            CoreError::PermissionDenied { .. } => "PERMISSION_DENIED".to_string(),
            CoreError::Internal { .. } => "INTERNAL".to_string(),
            CoreError::RateLimited { .. } => "RATE_LIMITED".to_string(),
            CoreError::RequestFailed { .. } => "REQUEST_FAILED".to_string(),
        }
    }
}

impl ErrorExt for RedditApiError {
    fn log_error(&self) -> &Self {
        error!("RedditApiError: {}", self);
        self
    }

    fn log_warn(&self) -> &Self {
        warn!("RedditApiError (warning): {}", self);
        self
    }

    fn is_retryable(&self) -> bool {
        match self {
            RedditApiError::RateLimitExceeded { .. } => true,
            RedditApiError::RequestTimeout => true,
            RedditApiError::ServerError { status_code } => *status_code >= 500,
            RedditApiError::EndpointUnavailable { .. } => true,
            _ => false,
        }
    }

    fn retry_after(&self) -> Option<Duration> {
        match self {
            RedditApiError::RateLimitExceeded { retry_after } => {
                Some(Duration::from_secs(*retry_after))
            }
            _ if self.is_retryable() => Some(Duration::from_secs(30)),
            _ => None,
        }
    }

    fn user_friendly_message(&self) -> String {
        match self {
            RedditApiError::AuthenticationFailed { .. } => {
                "Reddit authentication failed. Please check your credentials.".to_string()
            }
            RedditApiError::RateLimitExceeded { retry_after } => format!(
                "Too many requests. Please wait {} seconds before trying again.",
                retry_after
            ),
            RedditApiError::Forbidden { resource } => format!(
                "Access denied to {}. You may not have permission to view this content.",
                resource
            ),
            RedditApiError::SubredditNotFound { subreddit } => {
                format!("Subreddit '{}' not found or is private.", subreddit)
            }
            RedditApiError::PostNotFound { .. } => {
                "The requested post could not be found.".to_string()
            }
            RedditApiError::InvalidToken => {
                "Reddit authentication token is invalid. Please re-authenticate.".to_string()
            }
            RedditApiError::RequestTimeout => {
                "Request to Reddit timed out. Please try again.".to_string()
            }
            _ => "Reddit API error occurred. Please try again later.".to_string(),
        }
    }

    fn error_code(&self) -> String {
        match self {
            RedditApiError::AuthenticationFailed { .. } => "REDDIT_AUTH_FAILED".to_string(),
            RedditApiError::RateLimitExceeded { .. } => "REDDIT_RATE_LIMIT".to_string(),
            RedditApiError::Forbidden { .. } => "REDDIT_FORBIDDEN".to_string(),
            RedditApiError::SubredditNotFound { .. } => "REDDIT_SUBREDDIT_NOT_FOUND".to_string(),
            RedditApiError::PostNotFound { .. } => "REDDIT_POST_NOT_FOUND".to_string(),
            RedditApiError::InvalidToken => "REDDIT_INVALID_TOKEN".to_string(),
            RedditApiError::EndpointUnavailable { .. } => "REDDIT_ENDPOINT_UNAVAILABLE".to_string(),
            RedditApiError::RequestTimeout => "REDDIT_TIMEOUT".to_string(),
            RedditApiError::InvalidResponse { .. } => "REDDIT_INVALID_RESPONSE".to_string(),
            RedditApiError::ServerError { .. } => "REDDIT_SERVER_ERROR".to_string(),
        }
    }
}

impl ErrorExt for DatabaseError {
    fn log_error(&self) -> &Self {
        error!("DatabaseError: {}", self);
        self
    }

    fn log_warn(&self) -> &Self {
        warn!("DatabaseError (warning): {}", self);
        self
    }

    fn is_retryable(&self) -> bool {
        matches!(
            self,
            DatabaseError::DatabaseLocked
                | DatabaseError::ConnectionFailed { .. }
                | DatabaseError::TransactionFailed { .. }
        )
    }

    fn retry_after(&self) -> Option<Duration> {
        match self {
            DatabaseError::DatabaseLocked => Some(Duration::from_millis(100)),
            _ if self.is_retryable() => Some(Duration::from_secs(1)),
            _ => None,
        }
    }

    fn user_friendly_message(&self) -> String {
        match self {
            DatabaseError::ConnectionFailed { .. } => {
                "Database connection failed. Please try again.".to_string()
            }
            DatabaseError::DatabaseLocked => {
                "Database is temporarily busy. Please try again.".to_string()
            }
            DatabaseError::CorruptDatabase => {
                "Database appears to be corrupted. Please contact support.".to_string()
            }
            DatabaseError::InsufficientSpace => "Not enough storage space available.".to_string(),
            _ => "Database error occurred. Please try again.".to_string(),
        }
    }

    fn error_code(&self) -> String {
        match self {
            DatabaseError::ConnectionFailed { .. } => "DB_CONNECTION_FAILED".to_string(),
            DatabaseError::MigrationFailed { .. } => "DB_MIGRATION_FAILED".to_string(),
            DatabaseError::QueryFailed { .. } => "DB_QUERY_FAILED".to_string(),
            DatabaseError::TransactionFailed { .. } => "DB_TRANSACTION_FAILED".to_string(),
            DatabaseError::ConstraintViolation { .. } => "DB_CONSTRAINT_VIOLATION".to_string(),
            DatabaseError::DatabaseLocked => "DB_LOCKED".to_string(),
            DatabaseError::CorruptDatabase => "DB_CORRUPT".to_string(),
            DatabaseError::InsufficientSpace => "DB_INSUFFICIENT_SPACE".to_string(),
            DatabaseError::Sql(_) => "DB_SQL_ERROR".to_string(),
        }
    }
}

impl ErrorExt for LlmError {
    fn log_error(&self) -> &Self {
        error!("LlmError: {}", self);
        self
    }

    fn log_warn(&self) -> &Self {
        warn!("LlmError (warning): {}", self);
        self
    }

    fn is_retryable(&self) -> bool {
        matches!(
            self,
            LlmError::RateLimitExceeded { .. }
                | LlmError::ServiceUnavailable { .. }
                | LlmError::RequestTimeout { .. }
        )
    }

    fn retry_after(&self) -> Option<Duration> {
        match self {
            LlmError::RateLimitExceeded { retry_after, .. } => {
                Some(Duration::from_secs(*retry_after))
            }
            _ if self.is_retryable() => Some(Duration::from_secs(10)),
            _ => None,
        }
    }

    fn user_friendly_message(&self) -> String {
        match self {
            LlmError::AuthenticationFailed { provider } => format!(
                "Authentication failed for {}. Please check your API key.",
                provider
            ),
            LlmError::InvalidApiKey { provider } => format!(
                "Invalid API key for {}. Please update your credentials.",
                provider
            ),
            LlmError::RateLimitExceeded {
                provider,
                retry_after,
            } => format!(
                "Rate limit exceeded for {}. Please wait {} seconds.",
                provider, retry_after
            ),
            LlmError::ModelNotAvailable { model } => format!(
                "Model '{}' is not available. Please try a different model.",
                model
            ),
            LlmError::TokenLimitExceeded { max_tokens, .. } => {
                format!("Text is too long. Maximum {} tokens allowed.", max_tokens)
            }
            LlmError::ContentFiltered { .. } => {
                "Content was filtered by the AI provider's safety systems.".to_string()
            }
            LlmError::ServiceUnavailable { provider } => format!(
                "{} service is temporarily unavailable. Please try again later.",
                provider
            ),
            _ => "AI service error occurred. Please try again later.".to_string(),
        }
    }

    fn error_code(&self) -> String {
        match self {
            LlmError::AuthenticationFailed { .. } => "LLM_AUTH_FAILED".to_string(),
            LlmError::InvalidApiKey { .. } => "LLM_INVALID_API_KEY".to_string(),
            LlmError::RateLimitExceeded { .. } => "LLM_RATE_LIMIT".to_string(),
            LlmError::ModelNotAvailable { .. } => "LLM_MODEL_NOT_AVAILABLE".to_string(),
            LlmError::TokenLimitExceeded { .. } => "LLM_TOKEN_LIMIT".to_string(),
            LlmError::InvalidPrompt { .. } => "LLM_INVALID_PROMPT".to_string(),
            LlmError::ContentFiltered { .. } => "LLM_CONTENT_FILTERED".to_string(),
            LlmError::ServiceUnavailable { .. } => "LLM_SERVICE_UNAVAILABLE".to_string(),
            LlmError::RequestTimeout { .. } => "LLM_TIMEOUT".to_string(),
            LlmError::InsufficientCredits { .. } => "LLM_INSUFFICIENT_CREDITS".to_string(),
            LlmError::InvalidResponseFormat { .. } => "LLM_INVALID_RESPONSE".to_string(),
        }
    }
}

impl ErrorExt for EmbeddingError {
    fn log_error(&self) -> &Self {
        error!("EmbeddingError: {}", self);
        self
    }

    fn log_warn(&self) -> &Self {
        warn!("EmbeddingError (warning): {}", self);
        self
    }

    fn is_retryable(&self) -> bool {
        matches!(
            self,
            EmbeddingError::DownloadFailed { .. } | EmbeddingError::InferenceFailed { .. }
        )
    }

    fn retry_after(&self) -> Option<Duration> {
        if self.is_retryable() {
            Some(Duration::from_secs(2))
        } else {
            None
        }
    }

    fn user_friendly_message(&self) -> String {
        match self {
            EmbeddingError::ModelNotFound { model_name } => format!(
                "Embedding model '{}' not found. Please download it first.",
                model_name
            ),
            EmbeddingError::ModelLoadingFailed { .. } => {
                "Failed to load embedding model. Please try again.".to_string()
            }
            EmbeddingError::InsufficientMemory { required_mb } => format!(
                "Not enough memory to load model. {} MB required.",
                required_mb
            ),
            EmbeddingError::InputTooLong { max_tokens, .. } => {
                format!("Text is too long. Maximum {} tokens allowed.", max_tokens)
            }
            EmbeddingError::DownloadFailed { .. } => {
                "Failed to download embedding model. Please check your connection.".to_string()
            }
            _ => "Embedding processing error occurred. Please try again.".to_string(),
        }
    }

    fn error_code(&self) -> String {
        match self {
            EmbeddingError::ModelLoadingFailed { .. } => "EMBED_MODEL_LOAD_FAILED".to_string(),
            EmbeddingError::ModelNotFound { .. } => "EMBED_MODEL_NOT_FOUND".to_string(),
            EmbeddingError::TokenizationFailed { .. } => "EMBED_TOKENIZATION_FAILED".to_string(),
            EmbeddingError::InputTooLong { .. } => "EMBED_INPUT_TOO_LONG".to_string(),
            EmbeddingError::InferenceFailed { .. } => "EMBED_INFERENCE_FAILED".to_string(),
            EmbeddingError::UnsupportedFormat { .. } => "EMBED_UNSUPPORTED_FORMAT".to_string(),
            EmbeddingError::InsufficientMemory { .. } => "EMBED_INSUFFICIENT_MEMORY".to_string(),
            EmbeddingError::HardwareIncompatible { .. } => {
                "EMBED_HARDWARE_INCOMPATIBLE".to_string()
            }
            EmbeddingError::DownloadFailed { .. } => "EMBED_DOWNLOAD_FAILED".to_string(),
            EmbeddingError::DimensionMismatch { .. } => "EMBED_DIMENSION_MISMATCH".to_string(),
        }
    }
}

impl ErrorExt for ConfigError {
    fn log_error(&self) -> &Self {
        error!("ConfigError: {}", self);
        self
    }

    fn log_warn(&self) -> &Self {
        warn!("ConfigError (warning): {}", self);
        self
    }

    fn is_retryable(&self) -> bool {
        false // Config errors are typically not retryable
    }

    fn retry_after(&self) -> Option<Duration> {
        None
    }

    fn user_friendly_message(&self) -> String {
        match self {
            ConfigError::FileNotFound { .. } => {
                "Configuration file not found. Please check the installation.".to_string()
            }
            ConfigError::InvalidFormat { .. } => {
                "Configuration file format is invalid. Please check the settings.".to_string()
            }
            ConfigError::MissingField { field } => {
                format!("Required configuration field '{}' is missing.", field)
            }
            ConfigError::InvalidValue { field, .. } => {
                format!("Invalid value for configuration field '{}'.", field)
            }
            ConfigError::MissingEnvironmentVariable { var_name } => format!(
                "Environment variable '{}' is required but not set.",
                var_name
            ),
            ConfigError::PermissionDenied { .. } => {
                "Permission denied accessing configuration. Please check file permissions."
                    .to_string()
            }
            _ => "Configuration error occurred. Please check your settings.".to_string(),
        }
    }

    fn error_code(&self) -> String {
        match self {
            ConfigError::FileNotFound { .. } => "CONFIG_FILE_NOT_FOUND".to_string(),
            ConfigError::InvalidFormat { .. } => "CONFIG_INVALID_FORMAT".to_string(),
            ConfigError::MissingField { .. } => "CONFIG_MISSING_FIELD".to_string(),
            ConfigError::InvalidValue { .. } => "CONFIG_INVALID_VALUE".to_string(),
            ConfigError::MissingEnvironmentVariable { .. } => "CONFIG_MISSING_ENV_VAR".to_string(),
            ConfigError::ValidationFailed { .. } => "CONFIG_VALIDATION_FAILED".to_string(),
            ConfigError::InvalidEncryptionKey => "CONFIG_INVALID_ENCRYPTION_KEY".to_string(),
            ConfigError::VersionMismatch { .. } => "CONFIG_VERSION_MISMATCH".to_string(),
            ConfigError::PermissionDenied { .. } => "CONFIG_PERMISSION_DENIED".to_string(),
            ConfigError::Parse(_) => "CONFIG_PARSE_ERROR".to_string(),
        }
    }
}

pub struct ErrorReporter {
    report_errors: bool,
    report_warnings: bool,
}

impl ErrorReporter {
    pub fn new() -> Self {
        Self {
            report_errors: true,
            report_warnings: true,
        }
    }

    pub fn with_error_reporting(mut self, enabled: bool) -> Self {
        self.report_errors = enabled;
        self
    }

    pub fn with_warning_reporting(mut self, enabled: bool) -> Self {
        self.report_warnings = enabled;
        self
    }

    pub fn report_error(&self, error: &CoreError) {
        if self.report_errors {
            error.log_error();
            info!("Error code: {}", error.error_code());
            info!("User message: {}", error.user_friendly_message());
            if error.is_retryable() {
                if let Some(retry_after) = error.retry_after() {
                    info!("Error is retryable. Retry after: {:?}", retry_after);
                }
            }
        }
    }

    pub fn report_warning(&self, error: &CoreError) {
        if self.report_warnings {
            error.log_warn();
        }
    }
}

impl Default for ErrorReporter {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn retry_with_backoff<F, T, E>(
    mut operation: F,
    max_retries: usize,
    initial_delay: Duration,
) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
    E: ErrorExt,
{
    let mut attempt = 0;
    let mut delay = initial_delay;

    loop {
        match operation() {
            Ok(result) => return Ok(result),
            Err(error) => {
                if attempt >= max_retries || !error.is_retryable() {
                    return Err(error);
                }

                if let Some(retry_delay) = error.retry_after() {
                    delay = retry_delay;
                }

                info!(
                    "Retrying operation (attempt {}/{}) after {:?}",
                    attempt + 1,
                    max_retries,
                    delay
                );

                tokio::time::sleep(delay).await;
                delay = std::cmp::min(delay * 2, Duration::from_secs(60)); // Exponential backoff with max 60s
                attempt += 1;
            }
        }
    }
}
