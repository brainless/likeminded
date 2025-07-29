use likeminded_core::{CoreError, RedditApiError, RedditPost};
use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, RefreshToken, Scope,
    TokenResponse, TokenUrl,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use url::Url;

const REDDIT_AUTH_URL: &str = "https://www.reddit.com/api/v1/authorize";
const REDDIT_TOKEN_URL: &str = "https://www.reddit.com/api/v1/access_token";
const REDDIT_API_BASE: &str = "https://oauth.reddit.com";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: SystemTime,
    pub scope: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RedditOAuth2Config {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub user_agent: String,
}

impl RedditOAuth2Config {
    pub fn new(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
        user_agent: String,
    ) -> Self {
        Self {
            client_id,
            client_secret,
            redirect_uri,
            user_agent,
        }
    }
}

#[derive(Debug)]
pub enum AuthState {
    NotAuthenticated,
    PendingAuthorization {
        csrf_token: CsrfToken,
        pkce_verifier: PkceCodeVerifier,
    },
    Authenticated {
        token: RedditToken,
    },
    TokenExpired {
        token: RedditToken,
    },
}

pub struct RedditClient {
    config: RedditOAuth2Config,
    oauth_client: BasicClient,
    http_client: Client,
    auth_state: AuthState,
}

impl RedditClient {
    pub fn new(config: RedditOAuth2Config) -> Result<Self, CoreError> {
        let oauth_client = BasicClient::new(
            ClientId::new(config.client_id.clone()),
            Some(ClientSecret::new(config.client_secret.clone())),
            AuthUrl::new(REDDIT_AUTH_URL.to_string()).map_err(|e| {
                CoreError::Config(likeminded_core::ConfigError::InvalidValue {
                    field: "auth_url".to_string(),
                    value: e.to_string(),
                })
            })?,
            Some(TokenUrl::new(REDDIT_TOKEN_URL.to_string()).map_err(|e| {
                CoreError::Config(likeminded_core::ConfigError::InvalidValue {
                    field: "token_url".to_string(),
                    value: e.to_string(),
                })
            })?),
        )
        .set_redirect_uri(RedirectUrl::new(config.redirect_uri.clone()).map_err(|e| {
            CoreError::Config(likeminded_core::ConfigError::InvalidValue {
                field: "redirect_uri".to_string(),
                value: e.to_string(),
            })
        })?);

        let http_client = Client::builder()
            .user_agent(&config.user_agent)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| CoreError::Network(e))?;

        Ok(Self {
            config,
            oauth_client,
            http_client,
            auth_state: AuthState::NotAuthenticated,
        })
    }

    pub fn generate_auth_url(&mut self, scopes: &[&str]) -> Result<(String, CsrfToken), CoreError> {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let mut auth_request = self
            .oauth_client
            .authorize_url(CsrfToken::new_random)
            .set_pkce_challenge(pkce_challenge);

        // Add scopes
        for scope in scopes {
            auth_request = auth_request.add_scope(Scope::new(scope.to_string()));
        }

        // Reddit-specific parameters
        auth_request = auth_request.add_extra_param("duration", "permanent");

        let (auth_url, csrf_token) = auth_request.url();

        self.auth_state = AuthState::PendingAuthorization {
            csrf_token: csrf_token.clone(),
            pkce_verifier,
        };

        Ok((auth_url.to_string(), csrf_token))
    }

    pub async fn handle_callback(
        &mut self,
        callback_url: &str,
        expected_csrf: &CsrfToken,
    ) -> Result<RedditToken, CoreError> {
        tracing::debug!("Processing callback URL: {}", callback_url);
        let url = Url::parse(callback_url).map_err(|e| {
            tracing::error!("Failed to parse callback URL '{}': {}", callback_url, e);
            CoreError::RedditApi(RedditApiError::InvalidResponse {
                details: format!("Invalid callback URL: {}", e),
            })
        })?;

        let query_params: HashMap<String, String> = url.query_pairs().into_owned().collect();

        // Check for error parameter
        if let Some(error) = query_params.get("error") {
            return Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed {
                reason: error.clone(),
            }));
        }

        // Verify CSRF token
        let received_state = query_params.get("state").ok_or_else(|| {
            CoreError::RedditApi(RedditApiError::InvalidResponse {
                details: "Missing state parameter".to_string(),
            })
        })?;

        if received_state != expected_csrf.secret() {
            return Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed {
                reason: "CSRF token mismatch".to_string(),
            }));
        }

        // Get authorization code
        let auth_code = query_params.get("code").ok_or_else(|| {
            CoreError::RedditApi(RedditApiError::InvalidResponse {
                details: "Missing authorization code".to_string(),
            })
        })?;

        // Extract PKCE verifier from current state
        let pkce_verifier =
            match std::mem::replace(&mut self.auth_state, AuthState::NotAuthenticated) {
                AuthState::PendingAuthorization { pkce_verifier, .. } => pkce_verifier,
                _ => {
                    return Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed {
                        reason: "Invalid authentication state".to_string(),
                    }))
                }
            };

        // Clean the authorization code (Reddit sometimes adds trailing #_ characters)
        let cleaned_auth_code = auth_code.trim_end_matches("#_");
        tracing::debug!(
            "Original auth code: '{}', cleaned: '{}'",
            auth_code,
            cleaned_auth_code
        );

        // Exchange code for token
        tracing::debug!("Exchanging authorization code for token");

        // Create a custom HTTP client to debug the token exchange request and add User-Agent
        let user_agent = self.config.user_agent.clone();
        let custom_http_client = move |mut request: oauth2::HttpRequest| {
            let user_agent = user_agent.clone();
            Box::pin(async move {
                // Add User-Agent header - critical for Reddit API
                if let Ok(header_value) = user_agent.parse() {
                    request.headers.insert("user-agent", header_value);
                }

                tracing::debug!("Token exchange request:");
                tracing::debug!("  URL: {}", request.url);
                tracing::debug!("  Method: {}", request.method);
                tracing::debug!("  Headers: {:?}", request.headers);
                tracing::debug!("  Body: {}", String::from_utf8_lossy(&request.body));

                let response = async_http_client(request).await;

                match &response {
                    Ok(resp) => {
                        tracing::debug!("Token exchange response:");
                        tracing::debug!("  Status: {:?}", resp.status_code);
                        tracing::debug!("  Headers: {:?}", resp.headers);
                        tracing::debug!("  Body: {}", String::from_utf8_lossy(&resp.body));

                        // Check if Reddit returned an error page instead of JSON
                        if resp.status_code.as_u16() != 200 {
                            let body_text = String::from_utf8_lossy(&resp.body);
                            if body_text.contains("whoa there, pardner!") {
                                tracing::error!("Reddit blocked the request - likely due to User-Agent or rate limiting");
                                tracing::error!("Response body contains: {}", body_text);
                            } else if body_text.starts_with("<!doctype html>")
                                || body_text.starts_with("<html")
                            {
                                tracing::error!("Reddit returned HTML error page instead of JSON");
                                tracing::error!(
                                    "Status: {}, Body: {}",
                                    resp.status_code.as_u16(),
                                    body_text
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Token exchange HTTP error: {:?}", e);
                    }
                }

                response
            })
        };

        let token_result = self
            .oauth_client
            .exchange_code(AuthorizationCode::new(cleaned_auth_code.to_string()))
            .set_pkce_verifier(pkce_verifier)
            .request_async(custom_http_client)
            .await
            .map_err(|e| {
                tracing::error!("Token exchange failed: {:?}", e);
                CoreError::RedditApi(RedditApiError::AuthenticationFailed {
                    reason: format!("Token exchange failed: {}", e),
                })
            })?;

        let expires_at = SystemTime::now()
            + Duration::from_secs(
                token_result
                    .expires_in()
                    .map(|d| d.as_secs())
                    .unwrap_or(3600),
            );

        let scopes = token_result
            .scopes()
            .map(|scopes| scopes.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default();

        let token = RedditToken {
            access_token: token_result.access_token().secret().clone(),
            refresh_token: token_result.refresh_token().map(|t| t.secret().clone()),
            expires_at,
            scope: scopes,
        };

        self.auth_state = AuthState::Authenticated {
            token: token.clone(),
        };

        Ok(token)
    }

    pub async fn refresh_token(&mut self, refresh_token: &str) -> Result<RedditToken, CoreError> {
        let token_result = self
            .oauth_client
            .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
            .request_async(async_http_client)
            .await
            .map_err(|e| {
                CoreError::RedditApi(RedditApiError::AuthenticationFailed {
                    reason: format!("Token refresh failed: {}", e),
                })
            })?;

        let expires_at = SystemTime::now()
            + Duration::from_secs(
                token_result
                    .expires_in()
                    .map(|d| d.as_secs())
                    .unwrap_or(3600),
            );

        let scopes = token_result
            .scopes()
            .map(|scopes| scopes.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default();

        let new_token = RedditToken {
            access_token: token_result.access_token().secret().clone(),
            refresh_token: token_result
                .refresh_token()
                .map(|t| t.secret().clone())
                .or_else(|| Some(refresh_token.to_string())), // Keep old refresh token if new one not provided
            expires_at,
            scope: scopes,
        };

        self.auth_state = AuthState::Authenticated {
            token: new_token.clone(),
        };

        Ok(new_token)
    }

    pub fn set_token(&mut self, token: RedditToken) {
        let now = SystemTime::now();
        self.auth_state = if token.expires_at <= now {
            AuthState::TokenExpired { token }
        } else {
            AuthState::Authenticated { token }
        };
    }

    pub fn get_auth_state(&self) -> &AuthState {
        &self.auth_state
    }

    pub fn is_authenticated(&self) -> bool {
        matches!(self.auth_state, AuthState::Authenticated { .. })
    }

    pub fn needs_refresh(&self) -> bool {
        match &self.auth_state {
            AuthState::TokenExpired { .. } => true,
            AuthState::Authenticated { token } => {
                let now = SystemTime::now();
                // Check if token expires within next 5 minutes
                let buffer = Duration::from_secs(300);
                token.expires_at <= now + buffer
            }
            _ => false,
        }
    }

    pub async fn ensure_authenticated(&mut self) -> Result<(), CoreError> {
        let needs_refresh = self.needs_refresh();

        match &self.auth_state {
            AuthState::NotAuthenticated => {
                Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed {
                    reason: "Not authenticated. Please authenticate first.".to_string(),
                }))
            }
            AuthState::PendingAuthorization { .. } => {
                Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed {
                    reason: "Authentication pending. Please complete OAuth flow.".to_string(),
                }))
            }
            AuthState::Authenticated { token } => {
                if needs_refresh {
                    if let Some(refresh_token) = token.refresh_token.clone() {
                        self.refresh_token(&refresh_token).await?;
                    } else {
                        return Err(CoreError::RedditApi(RedditApiError::InvalidToken));
                    }
                }
                Ok(())
            }
            AuthState::TokenExpired { token } => {
                if let Some(refresh_token) = token.refresh_token.clone() {
                    self.refresh_token(&refresh_token).await?;
                    Ok(())
                } else {
                    Err(CoreError::RedditApi(RedditApiError::InvalidToken))
                }
            }
        }
    }

    pub fn get_required_scopes() -> Vec<&'static str> {
        vec![
            "identity",     // Access to user identity
            "read",         // Read access to posts and comments
            "mysubreddits", // Access to user's subreddit subscriptions
        ]
    }

    pub async fn fetch_posts(&mut self, subreddit: &str) -> Result<Vec<RedditPost>, CoreError> {
        self.fetch_posts_with_options(subreddit, None, None, Some(25), None)
            .await
    }

    pub async fn fetch_posts_with_options(
        &mut self,
        subreddit: &str,
        sort: Option<&str>,
        time_filter: Option<&str>,
        limit: Option<u32>,
        after: Option<&str>,
    ) -> Result<Vec<RedditPost>, CoreError> {
        self.ensure_authenticated().await?;

        if let AuthState::Authenticated { token } = &self.auth_state {
            let api_client = api::RedditApiClient::new(self.config.user_agent.clone());
            let listing = api_client
                .get_subreddit_posts_with_time_filter(
                    &token.access_token,
                    subreddit,
                    sort,
                    time_filter,
                    limit,
                    after,
                )
                .await?;

            let posts: Vec<RedditPost> = listing
                .data
                .children
                .into_iter()
                .map(|child| child.data.into())
                .collect();

            Ok(posts)
        } else {
            Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed {
                reason: "Not authenticated".to_string(),
            }))
        }
    }

    pub async fn fetch_multiple_subreddit_posts(
        &mut self,
        subreddits: &[&str],
        sort: Option<&str>,
        time_filter: Option<&str>,
        limit: Option<u32>,
        after: Option<&str>,
    ) -> Result<Vec<(String, Result<Vec<RedditPost>, CoreError>)>, CoreError> {
        self.ensure_authenticated().await?;

        if let AuthState::Authenticated { token } = &self.auth_state {
            let api_client = api::RedditApiClient::new(self.config.user_agent.clone());
            let results = api_client
                .get_multiple_subreddit_posts(
                    &token.access_token,
                    subreddits,
                    sort,
                    time_filter,
                    limit,
                    after,
                )
                .await?;

            // Convert API results to RedditPost results
            let converted_results: Vec<(String, Result<Vec<RedditPost>, CoreError>)> = results
                .into_iter()
                .map(|(subreddit, listing_result)| {
                    let posts_result = listing_result.map(|listing| {
                        listing
                            .data
                            .children
                            .into_iter()
                            .map(|child| child.data.into())
                            .collect()
                    });
                    (subreddit, posts_result)
                })
                .collect();

            Ok(converted_results)
        } else {
            Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed {
                reason: "Not authenticated".to_string(),
            }))
        }
    }

    pub async fn check_subreddit_access(&mut self, subreddit: &str) -> Result<bool, CoreError> {
        self.ensure_authenticated().await?;

        if let AuthState::Authenticated { token } = &self.auth_state {
            let api_client = api::RedditApiClient::new(self.config.user_agent.clone());
            api_client
                .check_subreddit_access(&token.access_token, subreddit)
                .await
        } else {
            Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed {
                reason: "Not authenticated".to_string(),
            }))
        }
    }

    pub async fn get_user_info(&mut self) -> Result<api::RedditUserData, CoreError> {
        self.ensure_authenticated().await?;

        if let AuthState::Authenticated { token } = &self.auth_state {
            let api_client = api::RedditApiClient::new(self.config.user_agent.clone());
            api_client.get_user_info(&token.access_token).await
        } else {
            Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed {
                reason: "Not authenticated".to_string(),
            }))
        }
    }

    pub async fn get_subreddit_info(
        &mut self,
        subreddit: &str,
    ) -> Result<api::RedditSubredditData, CoreError> {
        self.ensure_authenticated().await?;

        if let AuthState::Authenticated { token } = &self.auth_state {
            let api_client = api::RedditApiClient::new(self.config.user_agent.clone());
            api_client
                .get_subreddit_info(&token.access_token, subreddit)
                .await
        } else {
            Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed {
                reason: "Not authenticated".to_string(),
            }))
        }
    }

    pub async fn get_user_subreddits(
        &mut self,
    ) -> Result<Vec<api::RedditSubredditData>, CoreError> {
        self.ensure_authenticated().await?;

        if let AuthState::Authenticated { token } = &self.auth_state {
            let api_client = api::RedditApiClient::new(self.config.user_agent.clone());
            let listing = api_client
                .get_user_subreddits(&token.access_token, Some(100))
                .await?;

            let subreddits: Vec<api::RedditSubredditData> = listing
                .data
                .children
                .into_iter()
                .map(|child| child.data)
                .collect();

            Ok(subreddits)
        } else {
            Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed {
                reason: "Not authenticated".to_string(),
            }))
        }
    }

    pub async fn get_api_metrics(&self) -> metrics::ApiMetrics {
        let api_client = api::RedditApiClient::new(self.config.user_agent.clone());
        api_client.get_metrics().await
    }

    pub async fn get_rate_limit_status(&self) -> rate_limiter::RateLimitStatus {
        let api_client = api::RedditApiClient::new(self.config.user_agent.clone());
        api_client.get_rate_limit_status().await
    }
}

pub mod api;
pub mod api_tracker;
pub mod metrics;
pub mod rate_limiter;
pub mod request_queue;
pub mod usage_dashboard;

#[cfg(test)]
mod tests;
