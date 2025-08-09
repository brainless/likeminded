#[cfg(feature = "database")]
use crate::api_tracker::ApiTracker;
use crate::metrics::{MetricsCollector, RequestMetrics};
use crate::rate_limiter::{RateLimitConfig, RateLimiter};
use crate::retry::{RetryConfig, RetryExecutor};
use likeminded_core::{CoreError, RedditApiError, RedditPost};
use reqwest::{Client, Method, Response};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

const REDDIT_API_BASE: &str = "https://oauth.reddit.com";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditListing<T> {
    pub kind: String,
    pub data: RedditListingData<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditListingData<T> {
    pub children: Vec<RedditListingChild<T>>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub modhash: Option<String>,
    pub dist: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditListingChild<T> {
    pub kind: String,
    pub data: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditPostData {
    pub id: String,
    pub title: String,
    pub selftext: String,
    pub author: String,
    pub subreddit: String,
    pub subreddit_name_prefixed: String,
    pub url: String,
    pub permalink: String,
    pub created_utc: f64,
    pub score: i32,
    pub num_comments: u32,
    pub over_18: bool,
    pub stickied: bool,
    pub locked: bool,
    pub ups: i32,
    pub downs: i32,
    pub upvote_ratio: Option<f64>,
    pub thumbnail: Option<String>,
    pub is_self: bool,
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditUserData {
    pub id: String,
    pub name: String,
    pub created_utc: f64,
    pub link_karma: i32,
    pub comment_karma: i32,
    pub is_gold: bool,
    pub is_mod: bool,
    pub verified: bool,
    pub has_verified_email: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditSubredditData {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub title: String,
    pub description: String,
    pub subscribers: u32,
    pub active_user_count: Option<u32>,
    pub created_utc: f64,
    pub over18: bool,
    pub lang: String,
    pub url: String,
    pub icon_img: Option<String>,
    pub header_img: Option<String>,
}

#[derive(Debug)]
pub struct RedditApiClient {
    http_client: Client,
    rate_limiter: Arc<RateLimiter>,
    metrics: Arc<MetricsCollector>,
    retry_executor: Arc<RetryExecutor>,
    #[cfg(feature = "database")]
    api_tracker: Option<Arc<ApiTracker>>,
    #[cfg(not(feature = "database"))]
    #[allow(dead_code)]
    api_tracker: Option<()>, // Stub when database feature is disabled
    user_agent: String,
}

impl RedditApiClient {
    pub fn new(user_agent: String) -> Self {
        let rate_config = RateLimitConfig::reddit_oauth();
        let rate_limiter = Arc::new(RateLimiter::new(rate_config));
        let metrics = Arc::new(MetricsCollector::new());
        let retry_config = RetryConfig::reddit();
        let retry_executor = Arc::new(RetryExecutor::new(retry_config));

        let http_client = Client::builder()
            .user_agent(&user_agent)
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http_client,
            rate_limiter,
            metrics,
            retry_executor,
            api_tracker: None,
            user_agent,
        }
    }

    /// Create a new client with custom retry configuration
    pub fn with_retry_config(user_agent: String, retry_config: RetryConfig) -> Self {
        let rate_config = RateLimitConfig::reddit_oauth();
        let rate_limiter = Arc::new(RateLimiter::new(rate_config));
        let metrics = Arc::new(MetricsCollector::new());
        let retry_executor = Arc::new(RetryExecutor::new(retry_config));

        let http_client = Client::builder()
            .user_agent(&user_agent)
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http_client,
            rate_limiter,
            metrics,
            retry_executor,
            api_tracker: None,
            user_agent,
        }
    }

    #[cfg(feature = "database")]
    pub fn with_api_tracker(mut self, api_tracker: Arc<ApiTracker>) -> Self {
        self.api_tracker = Some(api_tracker);
        self
    }

    /// Make a request with retry logic
    pub async fn make_request(
        &self,
        method: Method,
        endpoint: &str,
        access_token: &str,
        query_params: Option<&[(&str, &str)]>,
    ) -> Result<Response, CoreError> {
        self.make_request_with_context(method, endpoint, access_token, query_params, None, None, 0)
            .await
    }

    pub async fn make_request_with_context(
        &self,
        method: Method,
        endpoint: &str,
        access_token: &str,
        query_params: Option<&[(&str, &str)]>,
        operation_type: Option<&str>,
        subreddit: Option<&str>,
        priority: i32,
    ) -> Result<Response, CoreError> {
        let operation_name = format!("{} {}", method, endpoint);

        // Clone values for use in closure
        let method_clone = method.clone();
        let endpoint_clone = endpoint.to_string();
        let access_token_clone = access_token.to_string();
        let query_params_clone = query_params.map(|params| params.to_vec());
        let operation_type_clone = operation_type.map(|s| s.to_string());
        let subreddit_clone = subreddit.map(|s| s.to_string());

        self.retry_executor
            .execute(&operation_name, || {
                let method = method_clone.clone();
                let endpoint = endpoint_clone.clone();
                let access_token = access_token_clone.clone();
                let query_params = query_params_clone.clone();
                let operation_type = operation_type_clone.clone();
                let subreddit = subreddit_clone.clone();

                async move {
                    self.make_request_internal(
                        method,
                        &endpoint,
                        &access_token,
                        query_params.as_deref(),
                        operation_type.as_deref(),
                        subreddit.as_deref(),
                        priority,
                    )
                    .await
                }
            })
            .await
    }

    /// Internal request method without retry logic
    async fn make_request_internal(
        &self,
        method: Method,
        endpoint: &str,
        access_token: &str,
        query_params: Option<&[(&str, &str)]>,
        #[cfg_attr(not(feature = "database"), allow(unused_variables))] operation_type: Option<
            &str,
        >,
        #[cfg_attr(not(feature = "database"), allow(unused_variables))] subreddit: Option<&str>,
        #[cfg_attr(not(feature = "database"), allow(unused_variables))] priority: i32,
    ) -> Result<Response, CoreError> {
        let url = format!("{}{}", REDDIT_API_BASE, endpoint);
        let start_time = Instant::now();
        let mut success = false;
        #[allow(unused_assignments)]
        let mut status_code = None;
        #[allow(unused_assignments)]
        let mut error_type = None;
        #[allow(unused_assignments)]
        let mut rate_limited = false;

        // Get rate limit status before request
        let rate_status_before = self.rate_limiter.get_rate_limit_status().await;
        #[cfg_attr(not(feature = "database"), allow(unused_variables))]
        let tokens_before = rate_status_before.available_tokens;

        // Acquire rate limit permit
        let permit = self.rate_limiter.acquire_permit().await;
        let queue_wait_time = permit.queue_wait_time;
        debug!(
            "Acquired rate limit permit for {} {} (waited {:?})",
            method, endpoint, queue_wait_time
        );

        // Build request
        let mut request_builder = self
            .http_client
            .request(method.clone(), &url)
            .bearer_auth(access_token)
            .header("User-Agent", &self.user_agent);

        if let Some(params) = query_params {
            request_builder = request_builder.query(params);
        }

        // Execute request
        info!("Making Reddit API request: {} {}", method, endpoint);
        let response = match request_builder.send().await {
            Ok(response) => {
                status_code = Some(response.status().as_u16());

                if response.status().is_success() {
                    success = true;
                    debug!("Request successful: {} {}", response.status(), endpoint);
                } else {
                    error!(
                        "Request failed with status: {} for {}",
                        response.status(),
                        endpoint
                    );

                    if response.status().as_u16() == 429 {
                        #[allow(unused_assignments)]
                        {
                            rate_limited = true;
                        }
                        #[allow(unused_assignments)]
                        {
                            error_type = Some("rate_limited".to_string());
                        }

                        // Extract retry-after header if present
                        if let Some(retry_after) = response.headers().get("retry-after") {
                            if let Ok(retry_seconds) =
                                retry_after.to_str().unwrap_or("60").parse::<u64>()
                            {
                                warn!("Rate limited, retry after {} seconds", retry_seconds);
                                return Err(CoreError::RedditApi(
                                    RedditApiError::RateLimitExceeded {
                                        retry_after: retry_seconds,
                                    },
                                ));
                            }
                        }

                        return Err(CoreError::RedditApi(RedditApiError::RateLimitExceeded {
                            retry_after: 60,
                        }));
                    } else if response.status().as_u16() == 401 {
                        #[allow(unused_assignments)]
                        {
                            error_type = Some("unauthorized".to_string());
                        }
                        return Err(CoreError::RedditApi(RedditApiError::InvalidToken));
                    } else if response.status().as_u16() == 403 {
                        #[allow(unused_assignments)]
                        {
                            error_type = Some("forbidden".to_string());
                        }
                        return Err(CoreError::RedditApi(RedditApiError::Forbidden {
                            resource: endpoint.to_string(),
                        }));
                    } else if response.status().as_u16() == 404 {
                        #[allow(unused_assignments)]
                        {
                            error_type = Some("not_found".to_string());
                        }
                        return Err(CoreError::RedditApi(RedditApiError::InvalidResponse {
                            details: "Resource not found".to_string(),
                        }));
                    } else if response.status().is_server_error() {
                        #[allow(unused_assignments)]
                        {
                            error_type = Some("server_error".to_string());
                        }
                        return Err(CoreError::RedditApi(RedditApiError::ServerError {
                            status_code: response.status().as_u16(),
                        }));
                    }
                }

                response
            }
            Err(e) => {
                error!("Network error for {} {}: {}", method, endpoint, e);
                #[allow(unused_assignments)]
                {
                    error_type = Some("network_error".to_string());
                }

                if e.is_timeout() {
                    return Err(CoreError::RedditApi(RedditApiError::RequestTimeout));
                } else {
                    return Err(CoreError::Network(e));
                }
            }
        };

        // Record metrics
        let response_time = start_time.elapsed();
        let request_metrics = RequestMetrics {
            endpoint: endpoint.to_string(),
            method: method.to_string(),
            status_code,
            response_time,
            success,
            rate_limited,
            error_type: error_type.clone(),
        };

        self.metrics.record_request(request_metrics).await;

        // Record API call in tracker if available
        #[cfg(feature = "database")]
        if let Some(ref tracker) = self.api_tracker {
            let rate_status_after = self.rate_limiter.get_rate_limit_status().await;
            let tokens_after = rate_status_after.available_tokens;

            if let Err(e) = tracker
                .record_api_call(
                    endpoint,
                    &method.to_string(),
                    status_code,
                    response_time,
                    rate_limited,
                    priority,
                    queue_wait_time,
                    operation_type,
                    subreddit,
                    Some(tokens_before),
                    Some(tokens_after),
                )
                .await
            {
                error!("Failed to record API call in tracker: {}", e);
            }
        }

        Ok(response)
    }

    pub async fn get_user_info(&self, access_token: &str) -> Result<RedditUserData, CoreError> {
        let response = self
            .make_request_with_context(
                Method::GET,
                "/api/v1/me",
                access_token,
                None,
                Some("get_user_info"),
                None,
                0,
            )
            .await?;

        let user_data: RedditUserData = response.json().await.map_err(|e| {
            error!("Failed to parse user data: {}", e);
            CoreError::RedditApi(RedditApiError::InvalidResponse {
                details: "Failed to parse user data".to_string(),
            })
        })?;

        debug!("Retrieved user info for: {}", user_data.name);
        Ok(user_data)
    }

    pub async fn get_subreddit_posts(
        &self,
        access_token: &str,
        subreddit: &str,
        sort: Option<&str>,
        limit: Option<u32>,
        after: Option<&str>,
    ) -> Result<RedditListing<RedditPostData>, CoreError> {
        self.get_subreddit_posts_with_time_filter(access_token, subreddit, sort, None, limit, after)
            .await
    }

    pub async fn get_subreddit_posts_with_time_filter(
        &self,
        access_token: &str,
        subreddit: &str,
        sort: Option<&str>,
        time_filter: Option<&str>,
        limit: Option<u32>,
        after: Option<&str>,
    ) -> Result<RedditListing<RedditPostData>, CoreError> {
        // Default to "hot" if no sort specified
        let sort_method = sort.unwrap_or("hot");

        // Validate sort method
        match sort_method {
            "hot" | "new" | "top" | "rising" | "controversial" => {}
            _ => {
                return Err(CoreError::RedditApi(RedditApiError::InvalidResponse {
                    details: format!("Invalid sort method: {}. Valid options: hot, new, top, rising, controversial", sort_method),
                }));
            }
        }

        let endpoint = format!("/r/{}/{}", subreddit, sort_method);
        let mut params = Vec::with_capacity(4);

        // Add limit parameter (default to 25, max 100)
        let actual_limit = limit.unwrap_or(25).min(100);
        let limit_str = actual_limit.to_string();
        params.push(("limit", limit_str.as_str()));

        // Add time filter for top/controversial sorts
        if let Some(time) = time_filter {
            if sort_method == "top" || sort_method == "controversial" {
                match time {
                    "hour" | "day" | "week" | "month" | "year" | "all" => {
                        params.push(("t", time));
                    }
                    _ => {
                        return Err(CoreError::RedditApi(RedditApiError::InvalidResponse {
                            details: format!("Invalid time filter: {}. Valid options: hour, day, week, month, year, all", time),
                        }));
                    }
                }
            }
        }

        // Add pagination parameter
        if let Some(after_val) = after {
            params.push(("after", after_val));
        }

        let query_params = if params.is_empty() {
            None
        } else {
            Some(params.as_slice())
        };

        let response = self
            .make_request_with_context(
                Method::GET,
                &endpoint,
                access_token,
                query_params,
                Some("get_subreddit_posts"),
                Some(subreddit),
                0,
            )
            .await?;

        let listing: RedditListing<RedditPostData> = response.json().await.map_err(|e| {
            error!("Failed to parse subreddit posts: {}", e);
            CoreError::RedditApi(RedditApiError::InvalidResponse {
                details: format!("Failed to parse posts for r/{}", subreddit),
            })
        })?;

        info!(
            "Retrieved {} posts from r/{} (sort: {}, limit: {})",
            listing.data.children.len(),
            subreddit,
            sort_method,
            actual_limit
        );
        Ok(listing)
    }

    /// Fetch posts from multiple subreddits concurrently
    pub async fn get_multiple_subreddit_posts(
        &self,
        access_token: &str,
        subreddits: &[&str],
        sort: Option<&str>,
        time_filter: Option<&str>,
        limit: Option<u32>,
        after: Option<&str>,
    ) -> Result<Vec<(String, Result<RedditListing<RedditPostData>, CoreError>)>, CoreError> {
        use futures::future::join_all;

        if subreddits.is_empty() {
            return Ok(vec![]);
        }

        info!("Fetching posts from {} subreddits", subreddits.len());

        // Create futures for all subreddit requests
        let futures = subreddits.iter().map(|subreddit| {
            let subreddit_name = subreddit.to_string();
            async move {
                let result = self
                    .get_subreddit_posts_with_time_filter(
                        access_token,
                        &subreddit_name,
                        sort,
                        time_filter,
                        limit,
                        after,
                    )
                    .await;
                (subreddit_name, result)
            }
        });

        // Execute all requests concurrently
        let results = join_all(futures).await;

        let success_count = results.iter().filter(|(_, result)| result.is_ok()).count();
        info!(
            "Successfully fetched posts from {}/{} subreddits",
            success_count,
            subreddits.len()
        );

        Ok(results)
    }

    /// Check if a subreddit exists and is accessible
    pub async fn check_subreddit_access(
        &self,
        access_token: &str,
        subreddit: &str,
    ) -> Result<bool, CoreError> {
        let endpoint = format!("/r/{}/about", subreddit);

        match self
            .make_request_with_context(
                Method::GET,
                &endpoint,
                access_token,
                None,
                Some("check_subreddit_access"),
                Some(subreddit),
                -1, // Lower priority for access checks
            )
            .await
        {
            Ok(_) => {
                debug!("Subreddit r/{} is accessible", subreddit);
                Ok(true)
            }
            Err(CoreError::RedditApi(RedditApiError::Forbidden { .. })) => {
                warn!("Subreddit r/{} is private or restricted", subreddit);
                Ok(false)
            }
            Err(CoreError::RedditApi(RedditApiError::SubredditNotFound { .. })) => {
                warn!("Subreddit r/{} does not exist", subreddit);
                Ok(false)
            }
            Err(e) => {
                error!("Error checking subreddit r/{}: {:?}", subreddit, e);
                Err(e)
            }
        }
    }

    pub async fn get_subreddit_info(
        &self,
        access_token: &str,
        subreddit: &str,
    ) -> Result<RedditSubredditData, CoreError> {
        let endpoint = format!("/r/{}/about", subreddit);

        let response = self
            .make_request_with_context(
                Method::GET,
                &endpoint,
                access_token,
                None,
                Some("get_subreddit_info"),
                Some(subreddit),
                0,
            )
            .await?;

        let subreddit_response: RedditListingChild<RedditSubredditData> =
            response.json().await.map_err(|e| {
                error!("Failed to parse subreddit info: {}", e);
                CoreError::RedditApi(RedditApiError::InvalidResponse {
                    details: format!("Failed to parse info for r/{}", subreddit),
                })
            })?;

        debug!("Retrieved info for r/{}", subreddit);
        Ok(subreddit_response.data)
    }

    pub async fn get_user_subreddits(
        &self,
        access_token: &str,
        limit: Option<u32>,
    ) -> Result<RedditListing<RedditSubredditData>, CoreError> {
        let endpoint = "/subreddits/mine/subscriber";
        let mut params = Vec::with_capacity(1);
        let limit_str = limit.map(|l| l.to_string());

        if let Some(ref limit_s) = limit_str {
            params.push(("limit", limit_s.as_str()));
        }

        let query_params = if params.is_empty() {
            None
        } else {
            Some(params.as_slice())
        };

        let response = self
            .make_request_with_context(
                Method::GET,
                endpoint,
                access_token,
                query_params,
                Some("get_user_subreddits"),
                None,
                1, // High priority for user data
            )
            .await?;

        let listing: RedditListing<RedditSubredditData> = response.json().await.map_err(|e| {
            error!("Failed to parse user subreddits: {}", e);
            CoreError::RedditApi(RedditApiError::InvalidResponse {
                details: "Failed to parse user subreddits".to_string(),
            })
        })?;

        info!("Retrieved {} user subreddits", listing.data.children.len());
        Ok(listing)
    }

    pub async fn get_metrics(&self) -> crate::metrics::ApiMetrics {
        self.metrics.get_metrics().await
    }

    pub async fn get_rate_limit_status(&self) -> crate::rate_limiter::RateLimitStatus {
        self.rate_limiter.get_rate_limit_status().await
    }

    pub async fn reset_metrics(&self) {
        self.metrics.reset_metrics().await;
    }

    /// Get retry metrics
    pub fn get_retry_metrics(&self) -> crate::retry::RetryMetrics {
        self.retry_executor.get_metrics()
    }

    /// Get circuit breaker state
    pub fn get_circuit_breaker_state(&self) -> crate::retry::CircuitBreakerState {
        self.retry_executor.get_circuit_breaker_state()
    }

    /// Reset retry metrics
    pub fn reset_retry_metrics(&self) {
        self.retry_executor.reset_metrics();
    }
}

// Helper function to convert RedditPostData to RedditPost
impl From<RedditPostData> for RedditPost {
    fn from(post_data: RedditPostData) -> Self {
        Self {
            id: post_data.id,
            title: post_data.title,
            content: if post_data.is_self && !post_data.selftext.is_empty() {
                Some(post_data.selftext)
            } else {
                None
            },
            subreddit: post_data.subreddit,
            url: post_data.url,
            permalink: format!("https://reddit.com{}", post_data.permalink),
            author: post_data.author,
            created_utc: post_data.created_utc as i64,
            score: post_data.score,
            num_comments: post_data.num_comments,
            upvote_ratio: post_data.upvote_ratio,
            over_18: post_data.over_18,
            stickied: post_data.stickied,
            locked: post_data.locked,
            is_self: post_data.is_self,
            domain: post_data.domain,
            thumbnail: post_data.thumbnail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_api_client_creation() {
        let client = RedditApiClient::new("test-user-agent/1.0".to_string());
        assert_eq!(client.user_agent, "test-user-agent/1.0");

        let status = client.get_rate_limit_status().await;
        assert!(status.available_tokens > 0);
    }

    #[tokio::test]
    async fn test_metrics_tracking() {
        let client = RedditApiClient::new("test-user-agent/1.0".to_string());

        // Initially no metrics
        let initial_metrics = client.get_metrics().await;
        assert_eq!(initial_metrics.total_requests, 0);

        // Reset should work
        client.reset_metrics().await;
        let reset_metrics = client.get_metrics().await;
        assert_eq!(reset_metrics.total_requests, 0);
    }

    #[test]
    fn test_reddit_post_conversion() {
        let post_data = RedditPostData {
            id: "test123".to_string(),
            title: "Test Post".to_string(),
            selftext: "This is test content".to_string(),
            author: "test_user".to_string(),
            subreddit: "test".to_string(),
            subreddit_name_prefixed: "r/test".to_string(),
            url: "https://reddit.com/r/test/comments/test123".to_string(),
            permalink: "/r/test/comments/test123".to_string(),
            created_utc: 1640995200.0,
            score: 42,
            num_comments: 5,
            over_18: false,
            stickied: false,
            locked: false,
            ups: 45,
            downs: 3,
            upvote_ratio: Some(0.93),
            thumbnail: None,
            is_self: true,
            domain: "self.test".to_string(),
        };

        let reddit_post: RedditPost = post_data.into();
        assert_eq!(reddit_post.id, "test123");
        assert_eq!(reddit_post.title, "Test Post");
        assert_eq!(
            reddit_post.content,
            Some("This is test content".to_string())
        );
    }
}
