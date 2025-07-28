#[cfg(test)]
mod tests {
    use crate::Database;
    use std::env;
    use tokio;

    async fn setup_test_db() -> Database {
        let db_path = env::temp_dir().join(format!("test_likeminded_{}.db", uuid::Uuid::new_v4()));
        let db_url = format!("sqlite://{}", db_path.display());

        let mut db = Database::new(db_url);
        db.connect()
            .await
            .expect("Failed to connect to test database");
        db.run_migrations().await.expect("Failed to run migrations");

        db
    }

    #[tokio::test]
    async fn test_database_connection_and_migrations() {
        let _db = setup_test_db().await;

        // If we get here, the database connection and migrations worked
        // This is a basic smoke test to ensure the database layer is functional
        assert!(true);
    }

    #[tokio::test]
    async fn test_basic_functionality() {
        let db = setup_test_db().await;

        // Test basic setting operations without using query macros
        db.save_setting("test_key", "test_value")
            .await
            .expect("Failed to save setting");
        let value = db
            .get_setting("test_key")
            .await
            .expect("Failed to get setting");
        assert_eq!(value, Some("test_value".to_string()));
    }
}
