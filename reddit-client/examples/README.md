# Reddit Client Examples

This directory contains examples for testing the Reddit API client functionality.

## Examples

### 1. Component Test (`component_test.rs`)
Tests basic functionality without requiring Reddit credentials:
```bash
cargo run --example component_test --package reddit-client
```

**What it tests:**
- ✅ Client creation
- ✅ Auth URL generation
- ✅ Rate limiting status
- ✅ API metrics collection
- ✅ Error handling without authentication

### 2. Manual Test (`manual_test.rs`)
Full interactive test requiring Reddit app credentials:
```bash
cargo run --example manual_test --package reddit-client
```

**What it tests:**
- 🔐 Complete OAuth2 authentication flow
- 👤 User info retrieval
- 📋 User's subreddits listing
- 📰 Posts from subreddits
- 📊 API metrics and rate limiting

## Setting Up Reddit App for Manual Testing

1. **Create Reddit App:**
   - Go to https://www.reddit.com/prefs/apps
   - Click "Create App" or "Create Another App"
   - Choose "web app"
   - Set redirect URI to: `http://localhost:8080/callback`
   - Save your **client ID** and **client secret**

2. **Run Manual Test:**
   ```bash
   cargo run --example manual_test --package reddit-client
   ```

3. **Follow Interactive Prompts:**
   - Enter your Reddit client ID and secret
   - Copy the generated authentication URL
   - Open it in your browser and authorize the app
   - Copy the callback URL from your browser (even if it shows an error page)
   - Paste it back into the terminal

## Expected Results

### Component Test Output:
```
=== Reddit Client Component Test ===

🧪 Test 1: Client Creation
✅ Client created successfully

🧪 Test 2: Auth URL Generation  
✅ Auth URL generated successfully

🧪 Test 3: Rate Limiting Status
✅ Rate limit status retrieved:
   Available tokens: 10/10
   Requests per minute limit: 100

🧪 Test 4: API Metrics
✅ API metrics retrieved:
   Total requests: 0

🧪 Test 5: Error Handling
✅ Correctly failed without authentication

🎉 Component test completed successfully!
```

### Manual Test Output:
```
=== Reddit API Manual Test ===

✅ Reddit client created successfully
🔗 Authentication URL generated
✅ Authentication successful!
👤 Getting user info... ✅ User info retrieved
📋 Getting user's subreddits... ✅ Found X subreddits
📰 Getting posts from r/rust... ✅ Found X posts
📊 API Metrics: Total requests: X
🚦 Rate Limit Status: Available tokens: X/10
🎉 Manual test completed successfully!
```

## Troubleshooting

### Common Issues:

1. **Invalid callback URL**: Make sure you copy the ENTIRE URL from the browser address bar after authorization
2. **Missing credentials**: Ensure you have valid Reddit app client ID and secret
3. **Network errors**: Check your internet connection and Reddit API status
4. **Rate limiting**: The client automatically handles rate limits with a 100 req/min limit

### Debug Output:
To see detailed logging, set the `RUST_LOG` environment variable:
```bash
RUST_LOG=debug cargo run --example manual_test --package reddit-client
```