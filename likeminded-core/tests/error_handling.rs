use likeminded_core::{
    ConfigError, CoreError, DatabaseError, EmbeddingError, ErrorExt, ErrorReporter, LlmError,
    RedditApiError,
};
use std::time::Duration;

#[test]
fn test_error_codes() {
    let reddit_error = CoreError::RedditApi(RedditApiError::InvalidToken);
    assert_eq!(reddit_error.error_code(), "REDDIT_API");

    let db_error = CoreError::Database(DatabaseError::DatabaseLocked);
    assert_eq!(db_error.error_code(), "DATABASE");

    let llm_error = CoreError::Llm(LlmError::InvalidApiKey {
        provider: "openai".to_string(),
    });
    assert_eq!(llm_error.error_code(), "LLM");

    let embedding_error = CoreError::Embedding(EmbeddingError::ModelNotFound {
        model_name: "bert".to_string(),
    });
    assert_eq!(embedding_error.error_code(), "EMBEDDING");

    let config_error = CoreError::Config(ConfigError::MissingField {
        field: "api_key".to_string(),
    });
    assert_eq!(config_error.error_code(), "CONFIG");
}

#[test]
fn test_retryable_errors() {
    let retryable_error =
        CoreError::RedditApi(RedditApiError::RateLimitExceeded { retry_after: 60 });
    assert!(retryable_error.is_retryable());

    let non_retryable_error = CoreError::Config(ConfigError::MissingField {
        field: "api_key".to_string(),
    });
    assert!(!non_retryable_error.is_retryable());
}

#[test]
fn test_retry_after() {
    let rate_limit_error =
        CoreError::RedditApi(RedditApiError::RateLimitExceeded { retry_after: 60 });
    assert_eq!(
        rate_limit_error.retry_after(),
        Some(Duration::from_secs(60))
    );

    let timeout_error = CoreError::Timeout { seconds: 30 };
    assert_eq!(timeout_error.retry_after(), Some(Duration::from_secs(30)));
}

#[test]
fn test_user_friendly_messages() {
    let reddit_error = CoreError::RedditApi(RedditApiError::InvalidToken);
    let message = reddit_error.user_friendly_message();
    assert!(!message.is_empty());
    assert!(message.contains("authentication token is invalid"));

    let config_error = CoreError::Config(ConfigError::MissingField {
        field: "api_key".to_string(),
    });
    let message = config_error.user_friendly_message();
    assert!(!message.is_empty());
    assert!(message.contains("api_key"));
}

#[test]
fn test_error_reporter() {
    let reporter = ErrorReporter::new()
        .with_error_reporting(true)
        .with_warning_reporting(true);
    let error = CoreError::RedditApi(RedditApiError::InvalidToken);

    // This test just ensures the methods don't panic
    reporter.report_error(&error);
    reporter.report_warning(&error);
}
