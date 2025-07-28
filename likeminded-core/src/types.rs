use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RedditPost {
    pub id: String,
    pub title: String,
    pub content: Option<String>,
    pub subreddit: String,
    pub url: String,
    pub created_utc: i64,
}

#[derive(Debug, Clone)]
pub struct Keyword {
    pub id: Option<i64>,
    pub text: String,
    pub embedding: Option<Vec<f32>>,
    pub created_at: i64,
}

#[derive(Debug)]
pub struct AppConfig {
    pub reddit_client_id: Option<String>,
    pub reddit_client_secret: Option<String>,
    pub llm_api_keys: HashMap<String, String>,
    pub polling_interval_minutes: u64,
}
