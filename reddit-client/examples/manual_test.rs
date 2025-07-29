use reddit_client::{RedditClient, RedditOAuth2Config};
use std::io::{self, Write};
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    println!("=== Reddit API Manual Test ===\n");

    // Get credentials from user
    println!("📋 Setup Instructions:");
    println!("1. Go to https://www.reddit.com/prefs/apps");
    println!("2. Create a new app (type: 'web app')");
    println!("3. Set redirect URI to: http://localhost:8080/callback");
    println!("4. Use the client ID and secret below\n");

    print!("Enter Reddit Client ID: ");
    io::stdout().flush()?;
    let mut client_id = String::new();
    io::stdin().read_line(&mut client_id)?;
    let client_id = client_id.trim().to_string();

    if client_id.is_empty() {
        println!("❌ Client ID cannot be empty. Please create a Reddit app first.");
        return Ok(());
    }

    print!("Enter Reddit Client Secret: ");
    io::stdout().flush()?;
    let mut client_secret = String::new();
    io::stdin().read_line(&mut client_secret)?;
    let client_secret = client_secret.trim().to_string();

    if client_secret.is_empty() {
        println!("❌ Client Secret cannot be empty. Please create a Reddit app first.");
        return Ok(());
    }

    // Create OAuth2 config
    let config = RedditOAuth2Config::new(
        client_id,
        client_secret,
        "http://localhost:8080/callback".to_string(),
        "likeminded/1.0 test app".to_string(),
    );

    // Create Reddit client
    let mut client = RedditClient::new(config)?;
    println!("✅ Reddit client created successfully\n");

    // Check initial authentication state
    println!("🔍 Initial authentication state: {:?}", client.get_auth_state());
    println!("🔍 Is authenticated: {}", client.is_authenticated());
    println!("🔍 Needs refresh: {}\n", client.needs_refresh());

    // Generate authentication URL
    let scopes = RedditClient::get_required_scopes();
    println!("📋 Required scopes: {:?}\n", scopes);

    let (auth_url, csrf_token) = client.generate_auth_url(&scopes)?;
    println!("🔗 Authentication URL generated:");
    println!("{}\n", auth_url);
    
    println!("🔍 Authentication state after URL generation: {:?}", client.get_auth_state());
    println!("🔒 CSRF Token: {}\n", csrf_token.secret());

    println!("📝 Instructions:");
    println!("1. Copy the URL above and open it in your browser");
    println!("2. Log in to Reddit and authorize the application");
    println!("3. You'll be redirected to localhost:8080/callback with a code");
    println!("4. Copy the ENTIRE callback URL and paste it here");
    println!("   Example: http://localhost:8080/callback?state=...&code=...\n");

    print!("Enter the callback URL: ");
    io::stdout().flush()?;
    let mut callback_url = String::new();
    io::stdin().read_line(&mut callback_url)?;
    let callback_url = callback_url.trim();
    
    if callback_url.is_empty() {
        println!("❌ Callback URL cannot be empty");
        return Ok(());
    }
    
    if !callback_url.starts_with("http://localhost:8080/callback") {
        println!("⚠️  Warning: URL doesn't look like a proper callback URL");
        println!("   Expected format: http://localhost:8080/callback?state=...&code=...");
    }

    // Handle OAuth callback
    println!("\n🔄 Processing OAuth callback...");
    match client.handle_callback(callback_url, &csrf_token).await {
        Ok(token) => {
            println!("✅ Authentication successful!");
            println!("🎫 Access token: {}...", &token.access_token[..20]);
            println!("🔄 Refresh token: {:?}", token.refresh_token.as_ref().map(|t| format!("{}...", &t[..20])));
            println!("⏰ Expires at: {:?}", token.expires_at);
            println!("📋 Scopes: {:?}\n", token.scope);
        }
        Err(e) => {
            println!("❌ Authentication failed: {}", e);
            return Ok(());
        }
    }

    // Test API access
    println!("🧪 Testing API access...\n");

    // Test 1: Get user info
    println!("👤 Getting user info...");
    match client.get_user_info().await {
        Ok(user) => {
            println!("✅ User info retrieved:");
            println!("   Name: {}", user.name);
            println!("   ID: {}", user.id);
            println!("   Link Karma: {}", user.link_karma);
            println!("   Comment Karma: {}", user.comment_karma);
            println!("   Created: {}", user.created_utc);
            println!("   Verified: {}\n", user.verified);
        }
        Err(e) => {
            println!("❌ Failed to get user info: {}\n", e);
        }
    }

    // Test 2: Get user's subreddits
    println!("📋 Getting user's subreddits...");
    match client.get_user_subreddits().await {
        Ok(subreddits) => {
            println!("✅ Found {} subreddits:", subreddits.len());
            for (i, sub) in subreddits.iter().take(5).enumerate() {
                println!("   {}. r/{} - {} subscribers", i + 1, sub.display_name, sub.subscribers);
            }
            if subreddits.len() > 5 {
                println!("   ... and {} more\n", subreddits.len() - 5);
            } else {
                println!();
            }
        }
        Err(e) => {
            println!("❌ Failed to get subreddits: {}\n", e);
        }
    }

    // Test 3: Get posts from a popular subreddit
    let test_subreddit = "rust";
    println!("📰 Getting posts from r/{}...", test_subreddit);
    match client.fetch_posts(test_subreddit).await {
        Ok(posts) => {
            println!("✅ Found {} posts:", posts.len());
            for (i, post) in posts.iter().take(3).enumerate() {
                println!("   {}. {} (Score: {})", i + 1, post.title, post.url);
                if let Some(ref content) = post.content {
                    let preview = if content.len() > 100 {
                        format!("{}...", &content[..100])
                    } else {
                        content.clone()
                    };
                    println!("      Preview: {}", preview);
                }
            }
            println!();
        }
        Err(e) => {
            println!("❌ Failed to get posts: {}\n", e);
        }
    }

    // Test 4: Check API metrics
    println!("📊 API Metrics:");
    let metrics = client.get_api_metrics().await;
    println!("   Total requests: {}", metrics.total_requests);
    println!("   Successful requests: {}", metrics.successful_requests);
    println!("   Failed requests: {}", metrics.failed_requests);
    println!("   Rate limited requests: {}", metrics.rate_limited_requests);
    println!("   Average response time: {:?}", metrics.average_response_time);

    // Test 5: Check rate limit status
    println!("\n🚦 Rate Limit Status:");
    let rate_status = client.get_rate_limit_status().await;
    println!("   Available tokens: {}/{}", rate_status.available_tokens, rate_status.max_tokens);
    println!("   Available permits: {}/{}", rate_status.available_permits, rate_status.max_permits);
    println!("   Requests per minute: {}", rate_status.requests_per_minute);
    println!("   Utilization: {:.1}%", rate_status.utilization_percentage());
    println!("   Near limit: {}", rate_status.is_near_limit());

    println!("\n🎉 Manual test completed successfully!");
    Ok(())
}