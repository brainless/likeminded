use likeminded_core::{CoreError, RedditPost};

pub trait LlmProvider {
    async fn analyze_post(&self, post: &RedditPost, keywords: &[String])
        -> Result<bool, CoreError>;
}

pub struct OpenAiProvider {
    api_key: String,
}

impl OpenAiProvider {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

impl LlmProvider for OpenAiProvider {
    async fn analyze_post(
        &self,
        _post: &RedditPost,
        _keywords: &[String],
    ) -> Result<bool, CoreError> {
        todo!("Implement OpenAI analysis")
    }
}

pub struct ClaudeProvider {
    api_key: String,
}

impl ClaudeProvider {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

impl LlmProvider for ClaudeProvider {
    async fn analyze_post(
        &self,
        _post: &RedditPost,
        _keywords: &[String],
    ) -> Result<bool, CoreError> {
        todo!("Implement Claude analysis")
    }
}
