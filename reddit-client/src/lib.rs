use likeminded_core::{CoreError, RedditPost};

pub struct RedditClient {
    client_id: String,
    client_secret: String,
}

impl RedditClient {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
        }
    }

    pub async fn authenticate(&self) -> Result<(), CoreError> {
        todo!("Implement OAuth2 authentication")
    }

    pub async fn fetch_posts(&self, subreddit: &str) -> Result<Vec<RedditPost>, CoreError> {
        todo!("Implement post fetching")
    }
}
