use likeminded_core::{AppConfig, CoreError, Keyword, RedditPost};
use sqlx::{migrate::MigrateDatabase, sqlite::SqlitePool, Sqlite};
use std::collections::HashMap;

pub struct Database {
    pool: Option<SqlitePool>,
    database_url: String,
}

#[derive(Debug, Clone)]
pub struct UserAction {
    pub id: Option<i64>,
    pub post_id: String,
    pub action_type: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct SubredditInfo {
    pub id: Option<i64>,
    pub name: String,
    pub is_active: bool,
    pub last_fetched_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Database {
    pub fn new(database_url: String) -> Self {
        Self {
            pool: None,
            database_url,
        }
    }

    pub async fn connect(&mut self) -> Result<(), CoreError> {
        // Create database if it doesn't exist
        if !Sqlite::database_exists(&self.database_url)
            .await
            .map_err(|e| CoreError::Configuration(format!("Database check failed: {}", e)))?
        {
            Sqlite::create_database(&self.database_url)
                .await
                .map_err(|e| {
                    CoreError::Configuration(format!("Database creation failed: {}", e))
                })?;
        }

        // Connect to database
        let pool = SqlitePool::connect(&self.database_url)
            .await
            .map_err(|e| CoreError::Configuration(format!("Database connection failed: {}", e)))?;

        self.pool = Some(pool);
        Ok(())
    }

    pub async fn run_migrations(&self) -> Result<(), CoreError> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| CoreError::Configuration("Database not connected".to_string()))?;

        let migration_sql = include_str!("../migrations/001_initial_schema.sql");

        sqlx::raw_sql(migration_sql)
            .execute(pool)
            .await
            .map_err(|e| CoreError::Configuration(format!("Migration failed: {}", e)))?;

        Ok(())
    }

    pub async fn save_post(&self, post: &RedditPost) -> Result<(), CoreError> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| CoreError::Configuration("Database not connected".to_string()))?;

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO posts (id, title, content, subreddit, url, author, score, created_utc, fetched_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            post.id,
            post.title,
            post.content,
            post.subreddit,
            post.url,
            "unknown", // We'll need to add author to RedditPost struct
            0, // We'll need to add score to RedditPost struct
            post.created_utc,
            chrono::Utc::now().timestamp()
        )
        .execute(pool)
        .await
        .map_err(|e| CoreError::Configuration(format!("Failed to save post: {}", e)))?;

        Ok(())
    }

    pub async fn get_posts(&self, limit: Option<i32>) -> Result<Vec<RedditPost>, CoreError> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| CoreError::Configuration("Database not connected".to_string()))?;

        let limit = limit.unwrap_or(50);
        let rows = sqlx::query!(
            "SELECT id, title, content, subreddit, url, created_utc 
             FROM posts 
             ORDER BY created_utc DESC 
             LIMIT ?",
            limit
        )
        .fetch_all(pool)
        .await
        .map_err(|e| CoreError::Configuration(format!("Failed to fetch posts: {}", e)))?;

        let posts = rows
            .into_iter()
            .map(|row| RedditPost {
                id: row.id,
                title: row.title,
                content: row.content,
                subreddit: row.subreddit,
                url: row.url,
                created_utc: row.created_utc,
            })
            .collect();

        Ok(posts)
    }

    pub async fn save_keyword(&self, keyword: &Keyword) -> Result<i64, CoreError> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| CoreError::Configuration("Database not connected".to_string()))?;

        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query!(
            r#"
            INSERT INTO keywords (text, created_at, updated_at)
            VALUES (?, ?, ?)
            "#,
            keyword.text,
            now,
            now
        )
        .execute(pool)
        .await
        .map_err(|e| CoreError::Configuration(format!("Failed to save keyword: {}", e)))?;

        Ok(result.last_insert_rowid())
    }

    pub async fn get_keywords(&self) -> Result<Vec<Keyword>, CoreError> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| CoreError::Configuration("Database not connected".to_string()))?;

        let rows = sqlx::query!(
            "SELECT id, text, embedding, created_at FROM keywords WHERE is_active = TRUE ORDER BY created_at DESC"
        )
        .fetch_all(pool)
        .await
        .map_err(|e| CoreError::Configuration(format!("Failed to fetch keywords: {}", e)))?;

        let keywords = rows
            .into_iter()
            .map(|row| {
                let embedding = if let Some(blob) = row.embedding {
                    // TODO: Deserialize embedding blob to Vec<f32>
                    Some(Vec::new()) // Placeholder
                } else {
                    None
                };

                Keyword {
                    id: Some(row.id),
                    text: row.text,
                    embedding,
                    created_at: row.created_at,
                }
            })
            .collect();

        Ok(keywords)
    }

    pub async fn save_setting(&self, key: &str, value: &str) -> Result<(), CoreError> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| CoreError::Configuration("Database not connected".to_string()))?;

        let now = chrono::Utc::now().timestamp();
        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO settings (key, value, created_at, updated_at)
            VALUES (?, ?, COALESCE((SELECT created_at FROM settings WHERE key = ?), ?), ?)
            "#,
            key,
            value,
            key,
            now,
            now
        )
        .execute(pool)
        .await
        .map_err(|e| CoreError::Configuration(format!("Failed to save setting: {}", e)))?;

        Ok(())
    }

    pub async fn get_setting(&self, key: &str) -> Result<Option<String>, CoreError> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| CoreError::Configuration("Database not connected".to_string()))?;

        let row = sqlx::query!("SELECT value FROM settings WHERE key = ?", key)
            .fetch_optional(pool)
            .await
            .map_err(|e| CoreError::Configuration(format!("Failed to fetch setting: {}", e)))?;

        Ok(row.map(|r| r.value))
    }

    pub async fn get_all_settings(&self) -> Result<HashMap<String, String>, CoreError> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| CoreError::Configuration("Database not connected".to_string()))?;

        let rows = sqlx::query!("SELECT key, value FROM settings")
            .fetch_all(pool)
            .await
            .map_err(|e| CoreError::Configuration(format!("Failed to fetch settings: {}", e)))?;

        let mut settings = HashMap::new();
        for row in rows {
            settings.insert(row.key, row.value);
        }

        Ok(settings)
    }

    pub async fn save_config(&self, config: &AppConfig) -> Result<(), CoreError> {
        if let Some(client_id) = &config.reddit_client_id {
            self.save_setting("reddit_client_id", client_id).await?;
        }
        if let Some(client_secret) = &config.reddit_client_secret {
            self.save_setting("reddit_client_secret", client_secret)
                .await?;
        }

        self.save_setting(
            "polling_interval_minutes",
            &config.polling_interval_minutes.to_string(),
        )
        .await?;

        // Save LLM API keys (encrypted storage would be implemented here)
        for (provider, key) in &config.llm_api_keys {
            // TODO: Implement encryption before saving
            self.save_api_key(provider, key).await?;
        }

        Ok(())
    }

    pub async fn get_config(&self) -> Result<AppConfig, CoreError> {
        let settings = self.get_all_settings().await?;

        let polling_interval_minutes = settings
            .get("polling_interval_minutes")
            .and_then(|s| s.parse().ok())
            .unwrap_or(15);

        // TODO: Decrypt API keys
        let llm_api_keys = HashMap::new();

        Ok(AppConfig {
            reddit_client_id: settings.get("reddit_client_id").cloned(),
            reddit_client_secret: settings.get("reddit_client_secret").cloned(),
            llm_api_keys,
            polling_interval_minutes,
        })
    }

    pub async fn save_api_key(&self, provider: &str, encrypted_key: &str) -> Result<(), CoreError> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| CoreError::Configuration("Database not connected".to_string()))?;

        let now = chrono::Utc::now().timestamp();
        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO api_keys (provider, encrypted_key, created_at, updated_at)
            VALUES (?, ?, COALESCE((SELECT created_at FROM api_keys WHERE provider = ?), ?), ?)
            "#,
            provider,
            encrypted_key.as_bytes(),
            provider,
            now,
            now
        )
        .execute(pool)
        .await
        .map_err(|e| CoreError::Configuration(format!("Failed to save API key: {}", e)))?;

        Ok(())
    }

    pub async fn record_user_action(
        &self,
        post_id: &str,
        action_type: &str,
    ) -> Result<(), CoreError> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| CoreError::Configuration("Database not connected".to_string()))?;

        let now = chrono::Utc::now().timestamp();
        sqlx::query!(
            "INSERT INTO user_actions (post_id, action_type, created_at) VALUES (?, ?, ?)",
            post_id,
            action_type,
            now
        )
        .execute(pool)
        .await
        .map_err(|e| CoreError::Configuration(format!("Failed to record user action: {}", e)))?;

        Ok(())
    }

    pub async fn get_active_subreddits(&self) -> Result<Vec<SubredditInfo>, CoreError> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| CoreError::Configuration("Database not connected".to_string()))?;

        let rows = sqlx::query!(
            "SELECT id, name, is_active, last_fetched_at, created_at, updated_at 
             FROM subreddits 
             WHERE is_active = TRUE 
             ORDER BY name"
        )
        .fetch_all(pool)
        .await
        .map_err(|e| CoreError::Configuration(format!("Failed to fetch subreddits: {}", e)))?;

        let subreddits = rows
            .into_iter()
            .map(|row| SubredditInfo {
                id: Some(row.id),
                name: row.name,
                is_active: row.is_active,
                last_fetched_at: row.last_fetched_at,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
            .collect();

        Ok(subreddits)
    }

    pub async fn update_subreddit_fetch_time(&self, subreddit: &str) -> Result<(), CoreError> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| CoreError::Configuration("Database not connected".to_string()))?;

        let now = chrono::Utc::now().timestamp();
        sqlx::query!(
            "UPDATE subreddits SET last_fetched_at = ?, updated_at = ? WHERE name = ?",
            now,
            now,
            subreddit
        )
        .execute(pool)
        .await
        .map_err(|e| {
            CoreError::Configuration(format!("Failed to update subreddit fetch time: {}", e))
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests;
