use reddit_client::{RedditClient, RedditOAuth2Config};
use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("=== Debug Callback URL Parsing ===\n");

    // Create a test client
    let config = RedditOAuth2Config::new(
        "test_client_id".to_string(),
        "test_client_secret".to_string(),
        "http://localhost:8080/callback".to_string(),
        "likeminded/1.0 test".to_string(),
    );

    let mut client = RedditClient::new(config)?;

    // Generate auth URL to get CSRF token
    let scopes = RedditClient::get_required_scopes();
    let (auth_url, csrf_token) = client.generate_auth_url(&scopes)?;
    
    println!("Generated auth URL: {}", auth_url);
    println!("CSRF token: {}\n", csrf_token.secret());

    // Test various callback URL formats
    let test_urls = vec![
        "http://localhost:8080/callback?code=test123&state=abc123",
        "http://localhost:8080/callback?error=access_denied&state=abc123",
        "http://localhost:8080/callback?code=test123", // Missing state
        "invalid-url", // Invalid URL format
    ];

    for (i, test_url) in test_urls.iter().enumerate() {
        println!("Test {}: {}", i + 1, test_url);
        match client.handle_callback(test_url, &csrf_token).await {
            Ok(token) => {
                println!("✅ Success: Got token {}", &token.access_token[..10]);
            }
            Err(e) => {
                println!("❌ Error: {}", e);
            }
        }
        println!();
    }

    // Interactive test
    println!("=== Interactive Test ===");
    println!("Paste your actual callback URL here:");
    print!("> ");
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    
    if !input.is_empty() {
        println!("Testing URL: {}", input);
        match client.handle_callback(input, &csrf_token).await {
            Ok(token) => {
                println!("✅ Success: Authentication completed!");
                println!("Token preview: {}...", &token.access_token[..20]);
            }
            Err(e) => {
                println!("❌ Error: {}", e);
            }
        }
    }

    Ok(())
}