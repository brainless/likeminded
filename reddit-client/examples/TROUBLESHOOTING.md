# Reddit OAuth2 Troubleshooting Guide

## Common Issues and Solutions

### "Failed to parse server response" Error

This error typically occurs during the OAuth2 token exchange step. Here are the most common causes and solutions:

#### 1. **Invalid Authorization Code**
**Symptoms:** Error message: "Token exchange failed: Failed to parse server response"

**Causes:**
- Authorization code has trailing `#_` characters (Reddit sometimes adds these)
- Code was copied incorrectly or is incomplete
- Code has expired (Reddit auth codes expire quickly)

**Solutions:**
- ✅ **Fixed in v1.1+**: The client now automatically strips trailing `#_` characters
- Copy the ENTIRE callback URL immediately after authorization
- Don't manually edit the authorization code

#### 2. **Incorrect App Configuration**
**Symptoms:** 400 Bad Request or parsing errors

**Causes:**
- Mismatch between app type in Reddit (Script vs Web App)
- Incorrect redirect URI in Reddit app settings
- Wrong client credentials

**Solutions:**
- Ensure your Reddit app is configured as a **Web App** (not Script)
- Verify redirect URI is exactly: `http://localhost:8080/callback`
- Double-check client ID and secret (no extra spaces/newlines)

#### 3. **Network/Firewall Issues**
**Symptoms:** Connection timeouts or unexpected responses

**Causes:**
- Corporate firewall blocking Reddit
- VPN interfering with requests
- Reddit API temporarily unavailable

**Solutions:**
- Try from a different network
- Disable VPN temporarily
- Check Reddit status at https://www.redditstatus.com/

### Common Error Messages

| Error | Cause | Solution |
|-------|-------|----------|
| `Invalid callback URL` | URL parsing failed | Ensure you copy the complete URL starting with `http://` |
| `CSRF token mismatch` | State parameter doesn't match | Use the same browser session, don't refresh |
| `Missing state parameter` | Incomplete callback URL | Copy the entire URL including query parameters |
| `Authentication failed: access_denied` | User denied permission | Re-authorize the application |
| `Client ID cannot be empty` | Missing Reddit app credentials | Create a Reddit app first |

### Debugging Steps

#### 1. **Enable Debug Logging**
```bash
RUST_LOG=debug cargo run --example manual_test --package reddit-client
```

#### 2. **Check Request Details**
The debug output will show:
- Generated auth URL
- CSRF token
- Authorization code processing
- Token exchange details

#### 3. **Verify Callback URL Format**
Correct format:
```
http://localhost:8080/callback?state=ABC123&code=XYZ789-qwerty
```

Common mistakes:
- Missing `http://` prefix
- Truncated parameters
- Extra characters from copy/paste

### Reddit App Setup Checklist

1. **Go to Reddit App Preferences**
   - Visit: https://www.reddit.com/prefs/apps
   - Click "Create App" or "Create Another App"

2. **Configure App Settings**
   - ✅ **Name**: Any descriptive name
   - ✅ **App type**: Select "web app" (NOT "script")
   - ✅ **Description**: Optional
   - ✅ **About URL**: Optional  
   - ✅ **Redirect URI**: `http://localhost:8080/callback`

3. **Save Credentials**
   - ✅ **Client ID**: The string under your app name
   - ✅ **Client Secret**: The "secret" field
   - ✅ Keep these secure and don't share them

### Testing Steps

#### 1. **Component Test (No credentials needed)**
```bash
cargo run --example component_test --package reddit-client
```
This verifies all components work without requiring Reddit API access.

#### 2. **Manual Authentication Test**
```bash
cargo run --example manual_test --package reddit-client
```
Full end-to-end test with your Reddit app credentials.

#### 3. **Debug Callback Parsing**
```bash
cargo run --example debug_callback --package reddit-client
```
Tests URL parsing and error handling.

### Still Having Issues?

If you continue experiencing problems:

1. **Check the logs** with `RUST_LOG=debug` for detailed error information
2. **Verify your Reddit app settings** match the requirements exactly
3. **Test with the component test** first to isolate the issue
4. **Try a different browser** or incognito mode
5. **Check Reddit API status** at https://www.redditstatus.com/

### Working Example Flow

```
1. User runs: cargo run --example manual_test --package reddit-client
2. Enters valid Reddit app credentials
3. Copies generated auth URL to browser
4. Logs in to Reddit and authorizes app
5. Copies callback URL from browser address bar
6. Pastes complete callback URL into terminal
7. ✅ Authentication succeeds, API calls work
```

The most important thing is to copy the **complete callback URL** exactly as it appears in your browser after authorization.