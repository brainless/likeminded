# Technical Overview - Likeminded

Cross-platform Rust GUI application for Reddit post filtering using LLM analysis.

## Architecture

### Core Components
- **GUI**: Iced framework with feed layout + left sidebar
- **Reddit API**: OAuth2 integration with rate limiting (100 req/min)
- **LLM Interface**: Trait-based abstraction for multiple providers
- **Embedding Engine**: Local Rust model using candle-transformers
- **Database**: SQLite for posts, keywords, API keys, user actions
- **Background Service**: System tray with configurable polling

### Key Modules

#### `reddit_client`
- OAuth2 authentication flow
- Subreddit post fetching
- Rate limit tracking and enforcement
- Error handling with exponential backoff

#### `llm_interface`
- Common trait for OpenAI, Claude, local models
- Provider switching at runtime
- API key management and validation

#### `embedding_engine`
- Local model download with progress tracking
- Text-to-vector conversion for posts and keywords
- Cosine similarity matching
- Small model preference (MiniLM variants)

#### `database`
- SQLite schema for posts, keywords, settings, api_keys
- User action tracking (read, good_match, not_good_match)
- Post metadata (subreddit, timestamp, confidence)

#### `gui`
- Main feed view with post cards
- Left sidebar for filters (subreddit, topics)
- Settings page for API keys and configuration
- Desktop notifications integration

#### `background_service`
- System tray integration
- Periodic post scanning respecting API limits
- New post notifications

## Data Flow

1. Background service polls subscribed subreddits
2. Posts processed through embedding similarity vs keywords
3. Matches stored in SQLite with metadata
4. GUI displays filtered feed with user actions
5. Rate limiting prevents API abuse

## Dependencies

- `iced` - GUI framework
- `reqwest` - HTTP client for Reddit API
- `sqlx` - SQLite database
- `candle-transformers` - Local embedding models
- `tokio` - Async runtime
- `serde` - JSON serialization
- `oauth2` - Reddit authentication
- `notify-rust` - Desktop notifications
- `tray-icon` - System tray integration

## Security Considerations

- API keys encrypted at rest in SQLite
- No sensitive data logged
- Local processing for privacy
- Reddit API compliance