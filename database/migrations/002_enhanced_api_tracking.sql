-- Enhanced API call tracking and rate limiting schema
-- This migration extends the existing reddit_api_stats table with additional tracking capabilities

-- Create table for detailed API call tracking with request priority and context
CREATE TABLE api_call_tracking (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint TEXT NOT NULL,                  -- Full API endpoint path
    method TEXT NOT NULL,                    -- HTTP method (GET, POST, etc.)
    status_code INTEGER,                     -- HTTP response status code
    response_time_ms INTEGER,                -- Response time in milliseconds
    request_size_bytes INTEGER,              -- Request payload size
    response_size_bytes INTEGER,             -- Response payload size
    rate_limited BOOLEAN NOT NULL DEFAULT FALSE, -- Whether this request was rate limited
    retry_after_seconds INTEGER,             -- Retry-After header value if rate limited
    error_type TEXT,                         -- Error classification if failed
    user_agent TEXT,                         -- User agent string used
    priority INTEGER NOT NULL DEFAULT 0,     -- Request priority (0=normal, 1=high, -1=low)
    queue_wait_time_ms INTEGER,              -- Time spent waiting in queue
    timestamp INTEGER NOT NULL,              -- Unix timestamp when request was made
    request_id TEXT,                         -- Unique identifier for request tracing
    
    -- Metadata for analysis
    subreddit TEXT,                          -- Subreddit if applicable
    operation_type TEXT,                     -- Operation type (fetch_posts, get_user_info, etc.)
    
    -- Rate limit context at time of request
    available_tokens_before INTEGER,         -- Available tokens before request
    available_tokens_after INTEGER,          -- Available tokens after request
    
    CHECK (timestamp > 0),
    CHECK (priority BETWEEN -1 AND 1)
);

-- Indexes for efficient querying
CREATE INDEX idx_api_call_tracking_timestamp ON api_call_tracking(timestamp);
CREATE INDEX idx_api_call_tracking_endpoint ON api_call_tracking(endpoint);
CREATE INDEX idx_api_call_tracking_rate_limited ON api_call_tracking(rate_limited);
CREATE INDEX idx_api_call_tracking_priority ON api_call_tracking(priority);
CREATE INDEX idx_api_call_tracking_operation ON api_call_tracking(operation_type);
CREATE INDEX idx_api_call_tracking_subreddit ON api_call_tracking(subreddit);

-- Create table for rate limit windows tracking
CREATE TABLE rate_limit_windows (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    window_start INTEGER NOT NULL,           -- Start of time window (Unix timestamp)
    window_end INTEGER NOT NULL,             -- End of time window (Unix timestamp)
    window_duration_seconds INTEGER NOT NULL, -- Duration of window in seconds
    request_count INTEGER NOT NULL DEFAULT 0, -- Number of requests in this window
    successful_requests INTEGER NOT NULL DEFAULT 0, -- Number of successful requests
    rate_limited_requests INTEGER NOT NULL DEFAULT 0, -- Number of rate limited requests
    total_response_time_ms INTEGER NOT NULL DEFAULT 0, -- Sum of all response times
    
    -- Rate limit status
    limit_reached BOOLEAN NOT NULL DEFAULT FALSE, -- Whether limit was reached in this window
    max_requests_allowed INTEGER NOT NULL,  -- Maximum requests allowed in this window
    
    -- Window metadata
    created_at INTEGER NOT NULL,             -- When this window record was created
    updated_at INTEGER NOT NULL,             -- Last update to this window
    
    CHECK (window_start < window_end),
    CHECK (request_count >= 0),
    CHECK (max_requests_allowed > 0),
    UNIQUE(window_start, window_duration_seconds)
);

-- Indexes for rate limit window queries
CREATE INDEX idx_rate_limit_windows_start ON rate_limit_windows(window_start);
CREATE INDEX idx_rate_limit_windows_end ON rate_limit_windows(window_end);
CREATE INDEX idx_rate_limit_windows_duration ON rate_limit_windows(window_duration_seconds);
CREATE INDEX idx_rate_limit_windows_limit_reached ON rate_limit_windows(limit_reached);

-- Create table for API usage alerts and warnings
CREATE TABLE api_usage_alerts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    alert_type TEXT NOT NULL,                -- Type: "approaching_limit", "limit_exceeded", "error_spike"
    severity TEXT NOT NULL,                  -- Severity: "info", "warning", "error", "critical"
    message TEXT NOT NULL,                   -- Human-readable alert message
    threshold_value REAL,                    -- Threshold that triggered the alert
    current_value REAL,                      -- Current value that exceeded threshold
    endpoint TEXT,                           -- Specific endpoint if applicable
    time_window_seconds INTEGER,             -- Time window for the alert
    triggered_at INTEGER NOT NULL,           -- When alert was triggered
    acknowledged_at INTEGER,                 -- When alert was acknowledged (NULL if not)
    resolved_at INTEGER,                     -- When condition was resolved (NULL if ongoing)
    
    -- Alert context
    context_data TEXT,                       -- JSON string with additional context
    action_taken TEXT,                       -- What action was taken in response
    
    CHECK (severity IN ('info', 'warning', 'error', 'critical')),
    CHECK (triggered_at > 0)
);

-- Indexes for alert queries
CREATE INDEX idx_api_usage_alerts_triggered_at ON api_usage_alerts(triggered_at);
CREATE INDEX idx_api_usage_alerts_severity ON api_usage_alerts(severity);
CREATE INDEX idx_api_usage_alerts_type ON api_usage_alerts(alert_type);
CREATE INDEX idx_api_usage_alerts_acknowledged ON api_usage_alerts(acknowledged_at);
CREATE INDEX idx_api_usage_alerts_resolved ON api_usage_alerts(resolved_at);

-- Create table for request queue management
CREATE TABLE request_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL UNIQUE,        -- Unique identifier for the request
    endpoint TEXT NOT NULL,                  -- API endpoint to call
    method TEXT NOT NULL,                    -- HTTP method
    priority INTEGER NOT NULL DEFAULT 0,     -- Priority level (-1=low, 0=normal, 1=high)
    operation_type TEXT,                     -- Type of operation for grouping
    
    -- Request details
    payload TEXT,                            -- Serialized request payload
    headers TEXT,                            -- Serialized headers
    query_params TEXT,                       -- Serialized query parameters
    
    -- Queue management
    queued_at INTEGER NOT NULL,              -- When request was queued
    scheduled_for INTEGER,                   -- When request is scheduled to execute
    started_at INTEGER,                      -- When request execution started
    completed_at INTEGER,                    -- When request completed
    failed_at INTEGER,                       -- When request failed (if applicable)
    
    -- Status tracking
    status TEXT NOT NULL DEFAULT 'queued',   -- Status: queued, executing, completed, failed, cancelled
    retry_count INTEGER NOT NULL DEFAULT 0,  -- Number of retry attempts
    max_retries INTEGER NOT NULL DEFAULT 3,  -- Maximum retry attempts
    
    -- Result
    response_data TEXT,                      -- Serialized response data
    error_message TEXT,                      -- Error message if failed
    
    CHECK (priority BETWEEN -1 AND 1),
    CHECK (status IN ('queued', 'executing', 'completed', 'failed', 'cancelled')),
    CHECK (retry_count >= 0),
    CHECK (queued_at > 0)
);

-- Indexes for queue management
CREATE INDEX idx_request_queue_status ON request_queue(status);
CREATE INDEX idx_request_queue_priority ON request_queue(priority, queued_at);
CREATE INDEX idx_request_queue_scheduled ON request_queue(scheduled_for);
CREATE INDEX idx_request_queue_operation ON request_queue(operation_type);
CREATE INDEX idx_request_queue_queued_at ON request_queue(queued_at);

-- Create table for API endpoint configurations
CREATE TABLE api_endpoint_configs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint_pattern TEXT NOT NULL UNIQUE,  -- Endpoint pattern (e.g., "/r/*/hot", "/api/v1/me")
    rate_limit_per_minute INTEGER NOT NULL, -- Rate limit for this endpoint
    rate_limit_per_hour INTEGER,            -- Hourly rate limit if different
    priority_weight REAL NOT NULL DEFAULT 1.0, -- Weight for priority calculations
    timeout_seconds INTEGER NOT NULL DEFAULT 30, -- Request timeout
    max_retries INTEGER NOT NULL DEFAULT 3,  -- Maximum retry attempts
    
    -- Endpoint metadata
    description TEXT,                        -- Human-readable description
    is_active BOOLEAN NOT NULL DEFAULT TRUE, -- Whether endpoint is active
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    
    CHECK (rate_limit_per_minute > 0),
    CHECK (priority_weight > 0),
    CHECK (timeout_seconds > 0),
    CHECK (max_retries >= 0)
);

-- Insert default endpoint configurations
INSERT INTO api_endpoint_configs (endpoint_pattern, rate_limit_per_minute, rate_limit_per_hour, description, created_at, updated_at) VALUES
    ('/api/v1/me', 10, 100, 'User info endpoint', strftime('%s', 'now'), strftime('%s', 'now')),
    ('/r/*/hot', 30, 1000, 'Subreddit hot posts', strftime('%s', 'now'), strftime('%s', 'now')),
    ('/r/*/new', 30, 1000, 'Subreddit new posts', strftime('%s', 'now'), strftime('%s', 'now')),
    ('/r/*/top', 30, 1000, 'Subreddit top posts', strftime('%s', 'now'), strftime('%s', 'now')),
    ('/r/*/about', 20, 500, 'Subreddit info', strftime('%s', 'now'), strftime('%s', 'now')),
    ('/subreddits/mine/subscriber', 5, 50, 'User subreddits', strftime('%s', 'now'), strftime('%s', 'now'));

-- Add new settings for enhanced rate limiting
INSERT OR REPLACE INTO settings (key, value, created_at, updated_at) VALUES
    ('rate_limit_enforcement_enabled', 'true', strftime('%s', 'now'), strftime('%s', 'now')),
    ('rate_limit_warning_threshold', '0.8', strftime('%s', 'now'), strftime('%s', 'now')),
    ('queue_max_size', '1000', strftime('%s', 'now'), strftime('%s', 'now')),
    ('queue_processing_enabled', 'true', strftime('%s', 'now'), strftime('%s', 'now')),
    ('api_usage_alerts_enabled', 'true', strftime('%s', 'now'), strftime('%s', 'now')),
    ('metrics_retention_days', '30', strftime('%s', 'now'), strftime('%s', 'now'));