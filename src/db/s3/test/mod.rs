#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::db::r2::R2Client;

    use bytes::Bytes;

    use tokio;

    async fn setup() -> R2Client {
        R2Client::new().await
    }

    fn load_test_image() -> Bytes {
        let path: PathBuf = ["images/pop_cat.webp"].iter().collect();
        std::fs::read(path)
            .expect("Failed to read test image")
            .into()
    }

    #[tokio::test]
    async fn test_upload_and_delete_thread_image_file() {
        let client = setup().await;
        let account_id = "test_account";
        let thread_id = 1;
        let content = load_test_image();
        let content_type = "image/webp";

        // Upload file
        let upload_result = client
            .upload_thread_image_file(account_id, thread_id, content.clone(), content_type)
            .await;
        assert!(upload_result.is_ok());
        let url = upload_result.unwrap();

        // Verify upload
        assert!(url.contains(&format!("thread:{}", thread_id)));
        assert!(url.contains(&format!("account:{}", account_id)));

        // Get file
        let key = format!("thread:{}/account:{}", thread_id, account_id);
        let get_result = client.get_file(&key).await;
        assert!(get_result.is_ok());
        assert_eq!(get_result.unwrap(), content);

        // Delete file
        let delete_result = client.delete_file(&key).await;
        assert!(delete_result.is_ok());

        // Verify deletion
        let get_after_delete = client.get_file(&key).await;
        assert!(get_after_delete.is_err());
    }

    #[tokio::test]
    async fn test_upload_and_delete_profile_image_file() {
        let client = setup().await;
        let account_id = "test_account_profile";
        let content = load_test_image();
        let content_type = "image/webp";

        // Upload file
        let upload_result = client
            .upload_profile_image_file(account_id, content.clone(), content_type)
            .await;
        assert!(upload_result.is_ok());
        let url = upload_result.unwrap();

        // Verify upload
        assert!(url.contains(&format!("profile:{}", account_id)));

        // Get file
        let key = format!("profile:{}", account_id);
        let get_result = client.get_file(&key).await;
        assert!(get_result.is_ok());
        assert_eq!(get_result.unwrap(), content);

        // Delete file
        let delete_result = client.delete_file(&key).await;
        assert!(delete_result.is_ok());

        // Verify deletion
        let get_after_delete = client.get_file(&key).await;
        assert!(get_after_delete.is_err());
    }
}
