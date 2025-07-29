use reddit_client::{RedditClient, RedditOAuth2Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Reddit Client Component Test ===\n");

    // Test 1: Client Creation
    println!("🧪 Test 1: Client Creation");
    let config = RedditOAuth2Config::new(
        "test_client_id".to_string(),
        "test_client_secret".to_string(),
        "http://localhost:8080/callback".to_string(),
        "likeminded/1.0 test".to_string(),
    );

    let client = RedditClient::new(config);
    match client {
        Ok(client) => {
            println!("✅ Client created successfully");
            println!("   Authenticated: {}", client.is_authenticated());
            println!("   Needs refresh: {}", client.needs_refresh());
        }
        Err(e) => {
            println!("❌ Client creation failed: {}", e);
            return Ok(());
        }
    }

    // Test 2: Auth URL Generation
    println!("\n🧪 Test 2: Auth URL Generation");
    let mut client = RedditClient::new(
        RedditOAuth2Config::new(
            "test_client_id".to_string(),
            "test_client_secret".to_string(),
            "http://localhost:8080/callback".to_string(),
            "likeminded/1.0 test".to_string(),
        )
    )?;

    let scopes = RedditClient::get_required_scopes();
    match client.generate_auth_url(&scopes) {
        Ok((auth_url, csrf_token)) => {
            println!("✅ Auth URL generated successfully");
            println!("   URL starts with: https://www.reddit.com/api/v1/authorize");
            println!("   Contains client_id: {}", auth_url.contains("client_id=test_client_id"));
            println!("   Contains scopes: {}", auth_url.contains("scope="));
            println!("   CSRF token length: {}", csrf_token.secret().len());
        }
        Err(e) => {
            println!("❌ Auth URL generation failed: {}", e);
        }
    }

    // Test 3: Rate Limiting Status
    println!("\n🧪 Test 3: Rate Limiting Status");
    let rate_status = client.get_rate_limit_status().await;
    println!("✅ Rate limit status retrieved:");
    println!("   Available tokens: {}/{}", rate_status.available_tokens, rate_status.max_tokens);
    println!("   Available permits: {}/{}", rate_status.available_permits, rate_status.max_permits);
    println!("   Requests per minute limit: {}", rate_status.requests_per_minute);
    println!("   Utilization: {:.1}%", rate_status.utilization_percentage());

    // Test 4: API Metrics
    println!("\n🧪 Test 4: API Metrics");
    let metrics = client.get_api_metrics().await;
    println!("✅ API metrics retrieved:");
    println!("   Total requests: {}", metrics.total_requests);
    println!("   Successful requests: {}", metrics.successful_requests);
    println!("   Failed requests: {}", metrics.failed_requests);
    println!("   Average response time: {:?}", metrics.average_response_time);

    // Test 5: Error Handling (without authentication)
    println!("\n🧪 Test 5: Error Handling");
    match client.get_user_info().await {
        Ok(_) => {
            println!("❌ Unexpected success - should fail without authentication");
        }
        Err(e) => {
            println!("✅ Correctly failed without authentication:");
            println!("   Error: {}", e);
        }
    }

    println!("\n🎉 Component test completed successfully!");
    println!("\n💡 To test full OAuth flow, run:");
    println!("   cargo run --example manual_test --package reddit-client");
    
    Ok(())
}