# Likeminded

Find Reddit posts that I am interested in

## Overview

I want to create a cross-platform Rust GUI application using Iced GUI framework. The app will use OAuth2 to log into my Reddit account and track posts that are interesting to me. I
will set some topics that I want to track. It should automatically search for posts in each of the subscribed sub-reddits and analyse for a match using LLMs. Please ask me questions
for clarifications, then write `TECHNICAL_OVERVIEW.md` for this project, keep it compact.

## Clarification Questions

1. LLM Integration: Which LLM service do you prefer? (OpenAI GPT, Anthropic Claude, local models like Ollama, or multiple options?)
2. Topic Definition: How do you want to define "interesting topics"? Keywords, natural language descriptions, or example posts?
3. Analysis Scope: Should the app analyze just post titles and content, or also comments? How deep should comment analysis go?
4. Subreddit Selection: Do you want to track all your subscribed subreddits, or manually select specific ones?
5. Real-time vs Batch: Should the app continuously monitor posts in real-time, or run periodic scans (hourly, daily)?
6. Storage & Privacy: Do you want matched posts stored locally, or just flagged/bookmarked on Reddit? Any privacy concerns with LLM API calls?
7. Notifications: What kind of alerts do you want when interesting posts are found? (Desktop notifications, in-app only, email?)
8. Scoring/Ranking: Should posts be ranked by relevance score, or just binary match/no-match?

## Clarification Answers

1. Multiple LLMs, ideally using a common inteface which defines most functions and prompts
2. Keywords that user can enter one by one
3. Only post titles and content for now
4. All my currently joined subreddits
5. Run as often as needed, staying within Reddit API limits, check official documentation
6. Stored locally on SQLite, user can click to open posts in their existing browser
7. In-app and desktop notifications
8. Just binary

## Additional Clarification Questions

9. **Reddit API Integration**: Do you have a Reddit application registered for OAuth2? What's your preferred approach for handling Reddit API rate limits (currently 100 requests per minute)?
10. **LLM Interface Design**: For the common LLM interface, should we support switching between providers at runtime, or configure one at startup? What's your preference for API key management?
11. **Keyword Matching Logic**: Should keywords use exact matching, fuzzy matching, or semantic similarity via embeddings? How should multiple keywords be combined (AND/OR logic)?
12. **Database Schema**: Besides matched posts, what metadata should we store? (timestamps, subreddit, match confidence, user actions like "mark as read"?)
13. **UI Layout**: Do you prefer a single-window design with tabs/panels, or multiple windows? Any specific preferences for the main view (list, cards, timeline)?
14. **Background Processing**: Should the app run in the system tray and work in the background, or only when actively opened?
15. **Error Handling**: How should the app handle API failures, network issues, or LLM service outages? Retry logic preferences?
16. **Configuration**: Should settings be managed through the GUI, config files, or both?

## Additiona Clarification Answers
9. Yes I have an application. Our app should simply stay within limits, maybe we keep track of our API calls
10. We should allow user to switch providers as long as the API keys exist. Keys would be entered in GUI and stored in DB
11. We should use embeddings with a local model, with a Rust based crate
12. Subreddit, match confidence (although ideally binary for now), user actions ("mark as read", "good match", "not good match")
13. Simple feed layout, with left sidebar for filter options (subreddit, topics, etc.), separate Settings page and left sidebar will change to items under Settings
14. Should run in the background
15. Good samaritan retry logic
16. Everything should be in GUI

## Final Questions

17. **Local Embedding Model**: Which Rust embedding crate/model do you prefer? (candle-transformers, ort, tch, or specific model like sentence-transformers/all-MiniLM-L6-v2?)

18. **Polling Frequency**: How often should the app check for new posts when running in background? (every 5 minutes, 15 minutes, hourly?)


## Clarifications

17. No preferences, small models to download would be better - need a basic model download status
18. As many times as API limits allow - we can set hard limits later
