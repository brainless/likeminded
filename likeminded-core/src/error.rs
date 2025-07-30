use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Reddit API error: {0}")]
    RedditApi(#[from] RedditApiError),

    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),

    #[error("Embedding error: {0}")]
    Embedding(#[from] EmbeddingError),

    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Invalid input: {message}")]
    InvalidInput { message: String },

    #[error("Operation timeout after {seconds} seconds")]
    Timeout { seconds: u64 },

    #[error("Resource not found: {resource}")]
    NotFound { resource: String },

    #[error("Permission denied: {operation}")]
    PermissionDenied { operation: String },

    #[error("Internal error: {message}")]
    Internal { message: String },

    #[error("Rate limited: {message}")]
    RateLimited {
        message: String,
        retry_after: Option<std::time::Duration>,
    },

    #[error("Request failed: {message}")]
    RequestFailed {
        message: String,
        status_code: Option<u16>,
    },
}

#[derive(Error, Debug, Clone)]
pub enum RedditApiError {
    #[error("Authentication failed: {reason}")]
    AuthenticationFailed { reason: String },

    #[error("Rate limit exceeded. Retry after {retry_after} seconds")]
    RateLimitExceeded { retry_after: u64 },

    #[error("Forbidden access to resource: {resource}")]
    Forbidden { resource: String },

    #[error("Subreddit not found: {subreddit}")]
    SubredditNotFound { subreddit: String },

    #[error("Post not found: {post_id}")]
    PostNotFound { post_id: String },

    #[error("Invalid OAuth token")]
    InvalidToken,

    #[error("API endpoint unavailable: {endpoint}")]
    EndpointUnavailable { endpoint: String },

    #[error("Request timeout")]
    RequestTimeout,

    #[error("Invalid API response: {details}")]
    InvalidResponse { details: String },

    #[error("Server error: {status_code}")]
    ServerError { status_code: u16 },
}

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Connection failed: {reason}")]
    ConnectionFailed { reason: String },

    #[error("Migration failed: {migration}")]
    MigrationFailed { migration: String },

    #[error("Query execution failed: {query}")]
    QueryFailed { query: String },

    #[error("Transaction failed: {reason}")]
    TransactionFailed { reason: String },

    #[error("Constraint violation: {constraint}")]
    ConstraintViolation { constraint: String },

    #[error("Database locked")]
    DatabaseLocked,

    #[error("Corrupt database")]
    CorruptDatabase,

    #[error("Insufficient storage space")]
    InsufficientSpace,

    #[error("SQL error: {0}")]
    Sql(#[from] sqlx::Error),
}

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("Provider authentication failed: {provider}")]
    AuthenticationFailed { provider: String },

    #[error("API key invalid or missing for {provider}")]
    InvalidApiKey { provider: String },

    #[error("Rate limit exceeded for {provider}. Retry after {retry_after} seconds")]
    RateLimitExceeded { provider: String, retry_after: u64 },

    #[error("Model not available: {model}")]
    ModelNotAvailable { model: String },

    #[error("Token limit exceeded. Max: {max_tokens}, requested: {requested_tokens}")]
    TokenLimitExceeded {
        max_tokens: u32,
        requested_tokens: u32,
    },

    #[error("Invalid prompt: {reason}")]
    InvalidPrompt { reason: String },

    #[error("Content filtered by provider: {reason}")]
    ContentFiltered { reason: String },

    #[error("Provider service unavailable: {provider}")]
    ServiceUnavailable { provider: String },

    #[error("Request timeout for {provider}")]
    RequestTimeout { provider: String },

    #[error("Insufficient credits for {provider}")]
    InsufficientCredits { provider: String },

    #[error("Invalid response format from {provider}")]
    InvalidResponseFormat { provider: String },
}

#[derive(Error, Debug)]
pub enum EmbeddingError {
    #[error("Model loading failed: {model_path}")]
    ModelLoadingFailed { model_path: String },

    #[error("Model not found: {model_name}")]
    ModelNotFound { model_name: String },

    #[error("Tokenization failed: {text_length} characters")]
    TokenizationFailed { text_length: usize },

    #[error("Input too long: {length} tokens, max: {max_tokens}")]
    InputTooLong { length: usize, max_tokens: usize },

    #[error("Model inference failed: {reason}")]
    InferenceFailed { reason: String },

    #[error("Unsupported model format: {format}")]
    UnsupportedFormat { format: String },

    #[error("Insufficient memory for model: {required_mb}MB required")]
    InsufficientMemory { required_mb: u64 },

    #[error("Hardware incompatible: {details}")]
    HardwareIncompatible { details: String },

    #[error("Model download failed: {url}")]
    DownloadFailed { url: String },

    #[error("Vector dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration file not found: {path}")]
    FileNotFound { path: String },

    #[error("Invalid configuration format: {details}")]
    InvalidFormat { details: String },

    #[error("Missing required field: {field}")]
    MissingField { field: String },

    #[error("Invalid value for {field}: {value}")]
    InvalidValue { field: String, value: String },

    #[error("Environment variable not set: {var_name}")]
    MissingEnvironmentVariable { var_name: String },

    #[error("Configuration validation failed: {reason}")]
    ValidationFailed { reason: String },

    #[error("Encryption key invalid or missing")]
    InvalidEncryptionKey,

    #[error("Configuration version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },

    #[error("Permission denied accessing config: {path}")]
    PermissionDenied { path: String },

    #[error("Configuration parsing error: {0}")]
    Parse(#[from] toml::de::Error),
}
