#[cfg(test)]
mod tests {
    use crate::pan123::entity;
    use crate::pan123::Pan123Client;
    use sea_orm::prelude::*;
    use sea_orm::EntityTrait;
    use tempfile::NamedTempFile;

    async fn setup_test_client() -> Pan123Client {
        let db_file = NamedTempFile::new().unwrap();
        let db_url = format!("sqlite:{}?mode=rwc", db_file.path().display());

        Pan123Client::new(
            "test_id".to_string(),
            "test_secret".to_string(),
            "/test_repo".to_string(),
            &db_url,
        )
        .await
        .expect("Failed to create client")
    }

    #[tokio::test]
    async fn test_db_init() {
        let client = setup_test_client().await;
        // Verify table exists by querying it
        let count = entity::Entity::find()
            .count(&client.db)
            .await
            .expect("Failed to query DB");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_list_files_empty() {
        let client = setup_test_client().await;
        let files = client.list_files(0).await.expect("Failed to list files");
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn test_find_path_id_not_found() {
        let client = setup_test_client().await;
        let id = client
            .find_path_id("/nonexistent")
            .await
            .expect("Failed to find path");
        assert!(id.is_none());
    }
}
