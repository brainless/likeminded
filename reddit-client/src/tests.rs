#[cfg(test)]
mod tests {
    use crate::{
        api, metrics, rate_limiter, AuthState, RedditClient, RedditOAuth2Config, RedditToken,
    };
    use likeminded_core::{CoreError, RedditApiError};
    use std::time::{Duration, SystemTime};

    fn create_test_config() -> RedditOAuth2Config {
        RedditOAuth2Config::new(
            "test_client_id".to_string(),
            "test_client_secret".to_string(),
            "http://localhost:8080/callback".to_string(),
            "likeminded/1.0 by test_user".to_string(),
        )
    }

    #[test]
    fn test_config_creation() {
        let config = create_test_config();
        assert_eq!(config.client_id, "test_client_id");
        assert_eq!(config.client_secret, "test_client_secret");
        assert_eq!(config.redirect_uri, "http://localhost:8080/callback");
        assert_eq!(config.user_agent, "likeminded/1.0 by test_user");
    }

    #[test]
    fn test_client_creation() {
        let config = create_test_config();
        let client = RedditClient::new(config);
        assert!(client.is_ok());

        let client = client.unwrap();
        assert!(!client.is_authenticated());
        assert!(!client.needs_refresh());
        assert!(matches!(
            client.get_auth_state(),
            AuthState::NotAuthenticated
        ));
    }

    #[test]
    fn test_auth_url_generation() {
        let config = create_test_config();
        let mut client = RedditClient::new(config).unwrap();

        let scopes = RedditClient::get_required_scopes();
        let result = client.generate_auth_url(&scopes);
        assert!(result.is_ok());

        let (auth_url, csrf_token) = result.unwrap();
        assert!(auth_url.contains("https://www.reddit.com/api/v1/authorize"));
        assert!(auth_url.contains("client_id=test_client_id"));
        assert!(auth_url.contains("redirect_uri=")); // Just check redirect_uri param exists
        assert!(auth_url.contains("scope=")); // Just check scope param exists
        assert!(auth_url.contains("duration=permanent"));
        assert!(!csrf_token.secret().is_empty());

        // Check that state changed to PendingAuthorization
        assert!(matches!(
            client.get_auth_state(),
            AuthState::PendingAuthorization { .. }
        ));
    }

    #[test]
    fn test_required_scopes() {
        let scopes = RedditClient::get_required_scopes();
        assert_eq!(scopes, vec!["identity", "read", "mysubreddits"]);
    }

    #[test]
    fn test_token_creation_and_expiry() {
        let now = SystemTime::now();
        let future = now + Duration::from_secs(3600);
        let past = now - Duration::from_secs(3600);

        let valid_token = RedditToken {
            access_token: "valid_token".to_string(),
            refresh_token: Some("refresh_token".to_string()),
            expires_at: future,
            scope: vec!["identity".to_string(), "read".to_string()],
        };

        let expired_token = RedditToken {
            access_token: "expired_token".to_string(),
            refresh_token: Some("refresh_token".to_string()),
            expires_at: past,
            scope: vec!["identity".to_string(), "read".to_string()],
        };

        let config = create_test_config();
        let mut client = RedditClient::new(config).unwrap();

        // Test setting valid token
        client.set_token(valid_token.clone());
        assert!(client.is_authenticated());
        assert!(!client.needs_refresh());

        // Test setting expired token
        client.set_token(expired_token.clone());
        assert!(!client.is_authenticated());
        assert!(client.needs_refresh());
        assert!(matches!(
            client.get_auth_state(),
            AuthState::TokenExpired { .. }
        ));
    }

    #[test]
    fn test_callback_url_parsing_errors() {
        let config = create_test_config();
        let mut client = RedditClient::new(config).unwrap();

        // Set up pending authorization state
        let scopes = RedditClient::get_required_scopes();
        let (_, csrf_token) = client.generate_auth_url(&scopes).unwrap();

        // Test invalid URL
        let result = tokio_test::block_on(client.handle_callback("not_a_url", &csrf_token));
        assert!(result.is_err());

        // Test error in callback
        let error_callback = "http://localhost:8080/callback?error=access_denied&state=test";
        let result = tokio_test::block_on(client.handle_callback(error_callback, &csrf_token));
        assert!(result.is_err());
        if let Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed { reason })) = result {
            assert_eq!(reason, "access_denied");
        } else {
            panic!("Expected AuthenticationFailed error");
        }

        // Test missing state
        let no_state_callback = "http://localhost:8080/callback?code=test_code";
        let result = tokio_test::block_on(client.handle_callback(no_state_callback, &csrf_token));
        assert!(result.is_err());

        // Test CSRF mismatch
        let wrong_state_callback =
            "http://localhost:8080/callback?code=test_code&state=wrong_state";
        let result =
            tokio_test::block_on(client.handle_callback(wrong_state_callback, &csrf_token));
        assert!(result.is_err());
        if let Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed { reason })) = result {
            assert_eq!(reason, "CSRF token mismatch");
        } else {
            panic!("Expected AuthenticationFailed error");
        }
    }

    #[tokio::test]
    async fn test_ensure_authenticated_states() {
        let config = create_test_config();
        let mut client = RedditClient::new(config).unwrap();

        // Test NotAuthenticated state
        let result = client.ensure_authenticated().await;
        assert!(result.is_err());
        if let Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed { reason })) = result {
            assert!(reason.contains("Not authenticated"));
        } else {
            panic!("Expected AuthenticationFailed error");
        }

        // Test PendingAuthorization state
        let scopes = RedditClient::get_required_scopes();
        client.generate_auth_url(&scopes).unwrap();

        let result = client.ensure_authenticated().await;
        assert!(result.is_err());
        if let Err(CoreError::RedditApi(RedditApiError::AuthenticationFailed { reason })) = result {
            assert!(reason.contains("Authentication pending"));
        } else {
            panic!("Expected AuthenticationFailed error");
        }
    }

    #[test]
    fn test_token_serialization() {
        let token = RedditToken {
            access_token: "test_access_token".to_string(),
            refresh_token: Some("test_refresh_token".to_string()),
            expires_at: SystemTime::UNIX_EPOCH + Duration::from_secs(1640995200), // Fixed timestamp
            scope: vec!["identity".to_string(), "read".to_string()],
        };

        // Test serialization
        let serialized = serde_json::to_string(&token).unwrap();
        assert!(serialized.contains("test_access_token"));
        assert!(serialized.contains("test_refresh_token"));

        // Test deserialization
        let deserialized: RedditToken = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.access_token, token.access_token);
        assert_eq!(deserialized.refresh_token, token.refresh_token);
        assert_eq!(deserialized.scope, token.scope);
    }

    // API Client Tests
    #[tokio::test]
    async fn test_api_client_creation() {
        let client = api::RedditApiClient::new("test-user-agent/1.0".to_string());
        let status = client.get_rate_limit_status().await;
        assert!(status.available_tokens > 0);
    }

    #[tokio::test]
    async fn test_api_metrics_integration() {
        let client = api::RedditApiClient::new("test-user-agent/1.0".to_string());

        // Check initial metrics
        let initial_metrics = client.get_metrics().await;
        assert_eq!(initial_metrics.total_requests, 0);

        // Reset metrics should work
        client.reset_metrics().await;
        let reset_metrics = client.get_metrics().await;
        assert_eq!(reset_metrics.total_requests, 0);
    }

    #[test]
    fn test_reddit_post_data_conversion() {
        let post_data = api::RedditPostData {
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

        let reddit_post: likeminded_core::RedditPost = post_data.into();
        assert_eq!(reddit_post.id, "test123");
        assert_eq!(reddit_post.title, "Test Post");
        assert_eq!(
            reddit_post.content,
            Some("This is test content".to_string())
        );
    }

    // Rate Limiter Tests
    #[tokio::test]
    async fn test_rate_limiter_status() {
        let config = rate_limiter::RateLimitConfig::reddit_oauth();
        let limiter = rate_limiter::RateLimiter::new(config);

        let status = limiter.get_rate_limit_status().await;
        assert!(status.available_tokens > 0);
        assert_eq!(status.max_tokens, 10);
        assert_eq!(status.requests_per_minute, 100);
    }

    #[tokio::test]
    async fn test_rate_limiter_permits() {
        let config = rate_limiter::RateLimitConfig::reddit_oauth();
        let limiter = rate_limiter::RateLimiter::new(config);

        // Should be able to acquire a permit
        let _permit = limiter.acquire_permit().await;

        // Check status after acquiring permit
        let status = limiter.get_rate_limit_status().await;
        assert!(status.available_tokens < 10);
    }

    // Metrics Tests
    #[tokio::test]
    async fn test_metrics_collector() {
        let collector = metrics::MetricsCollector::new();

        let request_metrics = metrics::RequestMetrics {
            endpoint: "/api/v1/me".to_string(),
            method: "GET".to_string(),
            status_code: Some(200),
            response_time: Duration::from_millis(150),
            success: true,
            rate_limited: false,
            error_type: None,
        };

        collector.record_request(request_metrics).await;

        let api_metrics = collector.get_metrics().await;
        assert_eq!(api_metrics.total_requests, 1);
        assert_eq!(api_metrics.successful_requests, 1);
        assert_eq!(api_metrics.failed_requests, 0);
        assert!(api_metrics.last_request_time.is_some());
    }

    #[tokio::test]
    async fn test_endpoint_specific_metrics() {
        let collector = metrics::MetricsCollector::new();

        let request_metrics = metrics::RequestMetrics {
            endpoint: "/r/rust/hot".to_string(),
            method: "GET".to_string(),
            status_code: Some(200),
            response_time: Duration::from_millis(200),
            success: true,
            rate_limited: false,
            error_type: None,
        };

        collector.record_request(request_metrics).await;

        let endpoint_metrics = collector.get_endpoint_metrics("/r/rust/hot").await;
        assert!(endpoint_metrics.is_some());

        let metrics = endpoint_metrics.unwrap();
        assert_eq!(metrics.request_count, 1);
        assert_eq!(metrics.success_count, 1);
        assert_eq!(metrics.success_rate(), 1.0);
        assert_eq!(metrics.average_response_time(), Duration::from_millis(200));
    }

    #[tokio::test]
    async fn test_integration_client_with_auth() {
        let config = create_test_config();
        let client = RedditClient::new(config).unwrap();

        // Test that API metrics are accessible through the client
        let metrics = client.get_api_metrics().await;
        assert_eq!(metrics.total_requests, 0);

        // Test rate limit status
        let status = client.get_rate_limit_status().await;
        assert!(status.available_tokens > 0);
    }
}
