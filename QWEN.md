# Likeminded - Qwen Overview

## Project Goal
A cross-platform Rust GUI application (using Iced framework) to filter Reddit posts based on user-defined keywords and LLM analysis. It will use OAuth2 for Reddit integration and store matched posts locally in SQLite.

## Development Workflow
- Create a new branch for each task
- Branch names should start with chore/ or feature/ or fix/
- Please add tests for any new features added, particularly integration tests
- Please run formatters, linters and tests before committing changes
- When finished please commit and push to the new branch
- Please mention GitHub issue if provided
- After working on an issue from GitHub, update issue's tasks and open PR

## Key Features
- **Reddit Integration**: OAuth2, fetching posts from subscribed subreddits, rate limit handling.
- **LLM Analysis**: Multiple LLM provider support via a common interface, local embedding model (Rust-based, e.g., `candle-transformers`) for semantic similarity matching of post titles/content against keywords.
- **Data Storage**: SQLite for posts, keywords, API keys, and user actions (read, good/not good match).
- **User Interface**: Simple feed layout with a left sidebar for filters and a separate settings page.
- **Background Processing**: Runs in the system tray, periodically scans for new posts, and provides in-app and desktop notifications.

## Architecture Highlights
- **GUI**: Iced framework.
- **LLM Interface**: Trait-based abstraction for runtime provider switching.
- **Embedding Engine**: Local Rust model for text-to-vector conversion and cosine similarity.
- **Database**: SQLite.
- **Background Service**: System tray integration with configurable polling.

## Core Dependencies
- `iced`: GUI framework
- `reqwest`: HTTP client
- `sqlx`: SQLite database
- `candle-transformers`: Local embedding models
- `tokio`: Async runtime
- `serde`: JSON serialization
- `oauth2`: Reddit authentication
- `notify-rust`: Desktop notifications
- `tray-icon`: System tray integration

## Security
- API keys encrypted at rest in SQLite.
- No sensitive data logging.
- Local processing for privacy.
- Reddit API compliance.