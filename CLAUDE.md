# Likeminded - Reddit Post Filter

## Project Overview
Cross-platform Rust GUI application using Iced framework to filter Reddit posts based on user-defined keywords using LLM analysis and local embeddings.

## Development Workflow
- Create a new branch for each task
- Branch names should start with chore/ or feature/ or fix/
- Please add tests for any new features added, particularly integration tests
- Please run formatters, linters and tests before committing changes
- When finished please commit and push to the new branch
- Please mention GitHub issue if provided
- After working on an issue from GitHub, update issue's tasks and open PR

## Key Technologies
- **Language**: Rust
- **GUI**: Iced framework
- **Database**: SQLite with sqlx
- **Reddit API**: OAuth2 authentication
- **LLM**: Multiple providers (OpenAI, Claude, local models)
- **Embeddings**: Local models via candle-transformers
- **Async**: Tokio runtime

## Architecture Components
- `reddit_client` - OAuth2 + API integration with rate limiting
- `llm_interface` - Trait-based abstraction for multiple LLM providers
- `embedding_engine` - Local text-to-vector conversion with similarity matching
- `database` - SQLite storage for posts, keywords, settings, user actions
- `gui` - Feed layout with sidebar filters and settings page
- `background_service` - System tray with periodic polling

## Key Features
- Background post monitoring within Reddit API limits (100 req/min)
- Local embedding-based keyword matching
- Desktop + in-app notifications
- User feedback system (mark as read, good/bad matches)
- Multi-LLM provider support with runtime switching
- System tray integration

## Development Commands
- `cargo fmt` - Format code
- `cargo clippy` - Run linter
- `cargo test` - Run tests
- `cargo check` - Quick compile check
- `cargo build` - Build project

## Claude Code Hooks
Pre-commit hooks are configured in `.claude/hooks/` to ensure code quality:
- `pre-commit-format` - Runs `cargo fmt --check` to ensure proper formatting
- `pre-commit-clippy` - Runs `cargo clippy` with warnings as errors for code quality
- `pre-commit-test` - Runs `cargo test` to ensure all tests pass

Use `git commit --no-verify` to bypass hooks if needed during development.

## Notes
- All configuration through GUI (no config files)
- API keys stored encrypted in SQLite
- Local model download with progress tracking
- Respects Reddit API rate limits with tracking
