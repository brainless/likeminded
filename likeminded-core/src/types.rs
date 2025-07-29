use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RedditPost {
    pub id: String,
    pub title: String,
    pub content: Option<String>,
    pub subreddit: String,
    pub url: String,
    pub permalink: String,
    pub author: String,
    pub created_utc: i64,
    pub score: i32,
    pub num_comments: u32,
    pub upvote_ratio: Option<f64>,
    pub over_18: bool,
    pub stickied: bool,
    pub locked: bool,
    pub is_self: bool,
    pub domain: String,
    pub thumbnail: Option<String>,
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
