-- Initial schema for Likeminded application
-- This migration creates all core tables for Reddit post filtering

-- Table: posts
-- Stores Reddit posts with metadata
CREATE TABLE posts (
    id TEXT PRIMARY KEY,              -- Reddit post ID (e.g., "t3_abc123")
    title TEXT NOT NULL,              -- Post title
    content TEXT,                     -- Post content/selftext (nullable for link posts)
    subreddit TEXT NOT NULL,          -- Subreddit name (without r/)
    url TEXT NOT NULL,                -- Reddit URL to the post
    author TEXT NOT NULL,             -- Reddit username of author
    score INTEGER NOT NULL DEFAULT 0, -- Post score/karma
    created_utc INTEGER NOT NULL,     -- Unix timestamp when post was created
    fetched_at INTEGER NOT NULL,      -- Unix timestamp when we fetched this post
    is_matched BOOLEAN NOT NULL DEFAULT FALSE, -- Whether this post matched our keywords
    match_confidence REAL,            -- Confidence score for the match (0.0-1.0)
    processed_at INTEGER,             -- When this post was processed by LLM/embeddings
    
    UNIQUE(id)
);

-- Index for efficient queries
CREATE INDEX idx_posts_subreddit ON posts(subreddit);
CREATE INDEX idx_posts_created_utc ON posts(created_utc);
CREATE INDEX idx_posts_is_matched ON posts(is_matched);
CREATE INDEX idx_posts_fetched_at ON posts(fetched_at);

-- Table: keywords
-- User-defined search keywords for filtering posts
CREATE TABLE keywords (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    text TEXT NOT NULL UNIQUE,        -- The keyword text
    embedding BLOB,                   -- Serialized embedding vector (nullable until computed)
    created_at INTEGER NOT NULL,      -- Unix timestamp when keyword was added
    updated_at INTEGER NOT NULL,      -- Unix timestamp when keyword was last updated
    is_active BOOLEAN NOT NULL DEFAULT TRUE, -- Whether this keyword is active for matching
    
    UNIQUE(text)
);

-- Index for efficient keyword lookups
CREATE INDEX idx_keywords_is_active ON keywords(is_active);
CREATE INDEX idx_keywords_created_at ON keywords(created_at);

-- Table: api_keys
-- Encrypted storage for LLM provider API keys
CREATE TABLE api_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL,           -- Provider name (e.g., "openai", "claude", "local")
    encrypted_key BLOB NOT NULL,      -- Encrypted API key
    created_at INTEGER NOT NULL,      -- Unix timestamp when key was added
    updated_at INTEGER NOT NULL,      -- Unix timestamp when key was last updated
    is_active BOOLEAN NOT NULL DEFAULT TRUE, -- Whether this key is currently active
    
    UNIQUE(provider)
);

-- Index for provider lookups
CREATE INDEX idx_api_keys_provider ON api_keys(provider);
CREATE INDEX idx_api_keys_is_active ON api_keys(is_active);

-- Table: settings
-- Application configuration settings
CREATE TABLE settings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key TEXT NOT NULL UNIQUE,         -- Setting key (e.g., "polling_interval", "active_llm_provider")
    value TEXT NOT NULL,              -- Setting value (JSON string for complex values)
    created_at INTEGER NOT NULL,      -- Unix timestamp when setting was created
    updated_at INTEGER NOT NULL,      -- Unix timestamp when setting was last updated
    
    UNIQUE(key)
);

-- Index for setting key lookups
CREATE INDEX idx_settings_key ON settings(key);

-- Table: user_actions
-- Track user interactions with posts for ML feedback
CREATE TABLE user_actions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    post_id TEXT NOT NULL,            -- Reference to posts.id
    action_type TEXT NOT NULL,        -- Type: "mark_read", "good_match", "not_good_match", "clicked"
    created_at INTEGER NOT NULL,      -- Unix timestamp when action was performed
    
    FOREIGN KEY (post_id) REFERENCES posts(id) ON DELETE CASCADE
);

-- Indexes for user actions
CREATE INDEX idx_user_actions_post_id ON user_actions(post_id);
CREATE INDEX idx_user_actions_action_type ON user_actions(action_type);
CREATE INDEX idx_user_actions_created_at ON user_actions(created_at);

-- Table: subreddits
-- Track which subreddits we're monitoring
CREATE TABLE subreddits (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,        -- Subreddit name (without r/)
    is_active BOOLEAN NOT NULL DEFAULT TRUE, -- Whether we're currently monitoring this subreddit
    last_fetched_at INTEGER,          -- Last time we fetched posts from this subreddit
    created_at INTEGER NOT NULL,      -- When we started monitoring this subreddit
    updated_at INTEGER NOT NULL,      -- Last update timestamp
    
    UNIQUE(name)
);

-- Index for subreddit queries
CREATE INDEX idx_subreddits_is_active ON subreddits(is_active);
CREATE INDEX idx_subreddits_last_fetched_at ON subreddits(last_fetched_at);

-- Table: reddit_api_stats
-- Track Reddit API usage to stay within limits
CREATE TABLE reddit_api_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint TEXT NOT NULL,           -- API endpoint called
    status_code INTEGER NOT NULL,     -- HTTP status code
    called_at INTEGER NOT NULL,       -- Unix timestamp of API call
    response_time_ms INTEGER,         -- Response time in milliseconds
    
    -- Keep only recent stats (last 24 hours for rate limiting)
    CHECK (called_at > 0)
);

-- Index for API rate limiting queries
CREATE INDEX idx_reddit_api_stats_called_at ON reddit_api_stats(called_at);
CREATE INDEX idx_reddit_api_stats_endpoint ON reddit_api_stats(endpoint);

-- Insert default settings
INSERT INTO settings (key, value, created_at, updated_at) VALUES
    ('polling_interval_minutes', '15', strftime('%s', 'now'), strftime('%s', 'now')),
    ('active_llm_provider', 'local', strftime('%s', 'now'), strftime('%s', 'now')),
    ('max_posts_per_fetch', '25', strftime('%s', 'now'), strftime('%s', 'now')),
    ('embedding_model_path', '', strftime('%s', 'now'), strftime('%s', 'now')),
    ('reddit_client_id', '', strftime('%s', 'now'), strftime('%s', 'now')),
    ('reddit_client_secret', '', strftime('%s', 'now'), strftime('%s', 'now'));

-- Insert default active subreddits (user can modify these later)
INSERT INTO subreddits (name, is_active, created_at, updated_at) VALUES
    ('rust', TRUE, strftime('%s', 'now'), strftime('%s', 'now')),
    ('programming', TRUE, strftime('%s', 'now'), strftime('%s', 'now')),
    ('MachineLearning', TRUE, strftime('%s', 'now'), strftime('%s', 'now'));