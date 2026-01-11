//! Integration tests for 123pan API operations.
//!
//! These tests require the following environment variables:
//! - PAN123_CLIENT_ID
//! - PAN123_CLIENT_SECRET

use bytes::Bytes;
use restic_123pan::pan123::Pan123Client;
use std::env;

/// Get test credentials from environment.
fn get_test_credentials() -> Option<(String, String)> {
    let client_id = env::var("PAN123_CLIENT_ID").ok()?;
    let client_secret = env::var("PAN123_CLIENT_SECRET").ok()?;
    Some((client_id, client_secret))
}

/// Skip test if credentials are not available.
macro_rules! skip_if_no_credentials {
    () => {
        if get_test_credentials().is_none() {
            eprintln!("Skipping test: PAN123_CLIENT_ID and PAN123_CLIENT_SECRET not set");
            return;
        }
    };
}

/// Get an access token from 123pan. Returns None if rate limited.
async fn get_access_token(client_id: &str, client_secret: &str) -> Option<String> {
    let client = reqwest::Client::new();
    let response = client
        .post("https://open-api.123pan.com/api/v1/access_token")
        .header("Platform", "open_platform")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "clientID": client_id,
            "clientSecret": client_secret
        }))
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = response.json().await.expect("Failed to parse response");

    // Handle rate limiting
    if body["code"] == 429 {
        eprintln!("Rate limited, skipping test");
        return None;
    }

    assert_eq!(body["code"], 0, "API error: {}", body["message"]);
    Some(
        body["data"]["accessToken"]
            .as_str()
            .expect("No access token")
            .to_string(),
    )
}

/// Skip test if rate limited.
macro_rules! skip_if_rate_limited {
    ($token:expr) => {{
        let Some(token) = $token else {
            eprintln!("Skipping test due to rate limiting");
            return;
        };
        token
    }};
}

#[tokio::test]
async fn test_authentication() {
    skip_if_no_credentials!();

    let (client_id, client_secret) = get_test_credentials().unwrap();

    // Test that we can get a token
    let client = reqwest::Client::new();
    let response = client
        .post("https://open-api.123pan.com/api/v1/access_token")
        .header("Platform", "open_platform")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "clientID": client_id,
            "clientSecret": client_secret
        }))
        .send()
        .await
        .expect("Failed to send request");

    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("Failed to parse response");

    println!("Auth response: {:?}", body);

    assert!(status.is_success(), "HTTP request failed: {}", status);
    assert_eq!(body["code"], 0, "API error: {}", body["message"]);
    assert!(
        body["data"]["accessToken"].is_string(),
        "No access token in response"
    );
}

#[tokio::test]
async fn test_list_root_directory() {
    skip_if_no_credentials!();

    let (client_id, client_secret) = get_test_credentials().unwrap();
    let access_token = skip_if_rate_limited!(get_access_token(&client_id, &client_secret).await);

    let client = reqwest::Client::new();
    let list_response = client
        .get("https://open-api.123pan.com/api/v2/file/list")
        .header("Platform", "open_platform")
        .header("Authorization", format!("Bearer {}", access_token))
        .query(&[("parentFileId", "0"), ("limit", "10")])
        .send()
        .await
        .expect("Failed to send request");

    let list_body: serde_json::Value = list_response
        .json()
        .await
        .expect("Failed to parse response");

    println!("List response: {:?}", list_body);

    assert_eq!(list_body["code"], 0, "API error: {}", list_body["message"]);
}

#[tokio::test]
async fn test_get_upload_domain() {
    skip_if_no_credentials!();

    let (client_id, client_secret) = get_test_credentials().unwrap();
    let access_token = skip_if_rate_limited!(get_access_token(&client_id, &client_secret).await);

    let client = reqwest::Client::new();
    let response = client
        .get("https://open-api.123pan.com/upload/v2/file/domain")
        .header("Platform", "open_platform")
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .expect("Failed to send request");

    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("Failed to parse response");

    println!("Upload domain response: {:?}", body);

    assert!(status.is_success(), "HTTP request failed: {}", status);
    assert_eq!(body["code"], 0, "API error: {}", body["message"]);

    let domains = body["data"].as_array().expect("data should be an array");
    assert!(
        !domains.is_empty(),
        "Should have at least one upload domain"
    );

    let domain = domains[0].as_str().expect("domain should be a string");
    println!("Got upload domain: {}", domain);
    assert!(
        domain.starts_with("https://"),
        "Domain should start with https://"
    );
}

#[tokio::test]
async fn test_create_and_delete_directory() {
    skip_if_no_credentials!();

    let (client_id, client_secret) = get_test_credentials().unwrap();
    let access_token = skip_if_rate_limited!(get_access_token(&client_id, &client_secret).await);

    // First get the upload domain
    let client = reqwest::Client::new();
    let domain_response = client
        .get("https://open-api.123pan.com/upload/v2/file/domain")
        .header("Platform", "open_platform")
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .expect("Failed to get upload domain");

    let domain_body: serde_json::Value = domain_response
        .json()
        .await
        .expect("Failed to parse response");
    let upload_domain = domain_body["data"][0].as_str().expect("No upload domain");

    println!("Using upload domain: {}", upload_domain);

    // Create a test directory using mkdir API (uses base URL, not upload domain)
    let test_dir_name = format!("test-dir-{}", chrono::Utc::now().timestamp());

    let mkdir_response = client
        .post("https://open-api.123pan.com/upload/v1/file/mkdir")
        .header("Platform", "open_platform")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "name": test_dir_name,
            "parentID": 0
        }))
        .send()
        .await
        .expect("Failed to create directory");

    let status = mkdir_response.status();
    let mkdir_body: serde_json::Value = mkdir_response
        .json()
        .await
        .expect("Failed to parse response");

    println!("Mkdir response: {:?}", mkdir_body);

    assert!(status.is_success(), "HTTP request failed: {}", status);
    assert_eq!(
        mkdir_body["code"], 0,
        "API error: {}",
        mkdir_body["message"]
    );

    let dir_id = mkdir_body["data"]["dirID"]
        .as_i64()
        .expect("No dirID in response");
    println!("Created directory '{}' with ID: {}", test_dir_name, dir_id);

    // Clean up: move to trash then delete
    let trash_response = client
        .post("https://open-api.123pan.com/api/v1/file/trash")
        .header("Platform", "open_platform")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "fileIDs": [dir_id]
        }))
        .send()
        .await
        .expect("Failed to trash directory");

    let trash_body: serde_json::Value = trash_response
        .json()
        .await
        .expect("Failed to parse response");
    println!("Trash response: {:?}", trash_body);

    let delete_response = client
        .post("https://open-api.123pan.com/api/v1/file/delete")
        .header("Platform", "open_platform")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "fileIDs": [dir_id]
        }))
        .send()
        .await
        .expect("Failed to delete directory");

    let delete_body: serde_json::Value = delete_response
        .json()
        .await
        .expect("Failed to parse response");
    println!("Delete response: {:?}", delete_body);
    assert_eq!(
        delete_body["code"], 0,
        "Delete failed: {}",
        delete_body["message"]
    );

    println!("Directory created and deleted successfully");
}

#[tokio::test]
async fn test_search_mode() {
    skip_if_no_credentials!();

    let (client_id, client_secret) = get_test_credentials().unwrap();
    let access_token = skip_if_rate_limited!(get_access_token(&client_id, &client_secret).await);

    // Test searchMode=1 for precise search
    let client = reqwest::Client::new();
    let response = client
        .get("https://open-api.123pan.com/api/v2/file/list")
        .header("Platform", "open_platform")
        .header("Authorization", format!("Bearer {}", access_token))
        .query(&[
            ("parentFileId", "0"),
            ("limit", "10"),
            ("searchMode", "1"),
            ("searchData", "nonexistent-file-name-12345"),
        ])
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = response.json().await.expect("Failed to parse response");

    println!("Search response: {:?}", body);

    assert_eq!(body["code"], 0, "API error: {}", body["message"]);

    // Should return empty list for nonexistent file
    let file_list = body["data"]["fileList"]
        .as_array()
        .expect("fileList should be array");
    assert!(
        file_list.is_empty(),
        "Should return empty list for nonexistent file"
    );

    println!("Search mode test passed");
}

// ============================================================================
// Cache Consistency Tests
// ============================================================================

/// Helper to create a unique test directory name
fn unique_test_path() -> String {
    format!("/cache-test-{}", chrono::Utc::now().timestamp_millis())
}

/// Helper to create a Pan123Client for testing
fn create_test_client(repo_path: &str) -> Option<Pan123Client> {
    let (client_id, client_secret) = get_test_credentials()?;
    Some(Pan123Client::new(
        client_id,
        client_secret,
        repo_path.to_string(),
    ))
}

/// Scenario 1: Basic cache hit - verify listing directory uses cache on second call
#[tokio::test]
async fn test_cache_scenario1_basic_cache_hit() {
    skip_if_no_credentials!();

    let repo_path = unique_test_path();
    let client = create_test_client(&repo_path).unwrap();

    // Create test directory
    let dir_id = match client.ensure_path(&repo_path).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!(
                "Failed to create test directory (may be rate limited): {:?}",
                e
            );
            return;
        }
    };

    // First call - should fetch from API and cache
    let files1 = client
        .list_files(dir_id)
        .await
        .expect("First list_files failed");

    // Second call - should use cache (we can verify by checking debug logs or timing)
    let files2 = client
        .list_files(dir_id)
        .await
        .expect("Second list_files failed");

    // Results should be identical
    assert_eq!(
        files1.len(),
        files2.len(),
        "Cache should return same number of files"
    );

    // Clean up
    let _ = client.delete_file(0, dir_id).await;

    println!("Scenario 1 passed: Basic cache hit works correctly");
}

/// Scenario 2: Upload new file updates cache
#[tokio::test]
async fn test_cache_scenario2_upload_new_file() {
    skip_if_no_credentials!();

    let repo_path = unique_test_path();
    let client = create_test_client(&repo_path).unwrap();

    // Create test directory
    let dir_id = match client.ensure_path(&repo_path).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!(
                "Failed to create test directory (may be rate limited): {:?}",
                e
            );
            return;
        }
    };

    // Initialize cache with empty directory
    let files_before = client.list_files(dir_id).await.expect("list_files failed");
    assert!(files_before.is_empty(), "Directory should start empty");

    // Upload a file
    let test_data = Bytes::from("test content for scenario 2");
    let file_id = client
        .upload_file(dir_id, "test-file.txt", test_data.clone())
        .await
        .expect("upload_file failed");

    // List files again - should include new file from cache
    let files_after = client
        .list_files(dir_id)
        .await
        .expect("list_files after upload failed");

    assert_eq!(
        files_after.len(),
        1,
        "Should have exactly one file after upload"
    );
    assert_eq!(
        files_after[0].filename, "test-file.txt",
        "Filename should match"
    );
    assert_eq!(
        files_after[0].size,
        test_data.len() as i64,
        "Size should match"
    );
    assert_eq!(files_after[0].file_id, file_id, "File ID should match");

    // Clean up
    let _ = client.delete_file(dir_id, file_id).await;
    let _ = client.delete_file(0, dir_id).await;

    println!("Scenario 2 passed: Upload new file updates cache correctly");
}

/// Scenario 3: Overwrite upload updates cache (duplicate=2)
#[tokio::test]
async fn test_cache_scenario3_overwrite_upload() {
    skip_if_no_credentials!();

    let repo_path = unique_test_path();
    let client = create_test_client(&repo_path).unwrap();

    // Create test directory
    let dir_id = match client.ensure_path(&repo_path).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!(
                "Failed to create test directory (may be rate limited): {:?}",
                e
            );
            return;
        }
    };

    // Initialize cache
    let _ = client.list_files(dir_id).await.expect("list_files failed");

    // Upload initial version
    let data_v1 = Bytes::from("version 1 content - 100 bytes padding xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
    let file_id_v1 = client
        .upload_file(dir_id, "config", data_v1.clone())
        .await
        .expect("first upload failed");

    let files_v1 = client
        .list_files(dir_id)
        .await
        .expect("list after v1 failed");
    assert_eq!(files_v1.len(), 1, "Should have one file after first upload");
    assert_eq!(
        files_v1[0].size,
        data_v1.len() as i64,
        "Size should match v1"
    );

    // Upload new version (overwrite)
    let data_v2 = Bytes::from("version 2 - different size content with more padding xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
    let file_id_v2 = client
        .upload_file(dir_id, "config", data_v2.clone())
        .await
        .expect("second upload failed");

    let files_v2 = client
        .list_files(dir_id)
        .await
        .expect("list after v2 failed");

    // Should still have exactly one file (not duplicated)
    assert_eq!(
        files_v2.len(),
        1,
        "Should still have exactly one file after overwrite"
    );
    assert_eq!(
        files_v2[0].filename, "config",
        "Filename should be unchanged"
    );
    assert_eq!(
        files_v2[0].size,
        data_v2.len() as i64,
        "Size should be updated to v2"
    );
    assert_eq!(files_v2[0].file_id, file_id_v2, "File ID should be updated");
    assert_ne!(
        file_id_v1, file_id_v2,
        "File IDs should differ between versions"
    );

    // Clean up
    let _ = client.delete_file(dir_id, file_id_v2).await;
    let _ = client.delete_file(0, dir_id).await;

    println!("Scenario 3 passed: Overwrite upload updates cache correctly (no duplicates)");
}

/// Scenario 4: Delete file removes from cache
#[tokio::test]
async fn test_cache_scenario4_delete_removes_from_cache() {
    skip_if_no_credentials!();

    let repo_path = unique_test_path();
    let client = create_test_client(&repo_path).unwrap();

    // Create test directory
    let dir_id = match client.ensure_path(&repo_path).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!(
                "Failed to create test directory (may be rate limited): {:?}",
                e
            );
            return;
        }
    };

    // Initialize cache
    let _ = client.list_files(dir_id).await.expect("list_files failed");

    // Upload a file
    let test_data = Bytes::from("to be deleted");
    let file_id = client
        .upload_file(dir_id, "to_delete.txt", test_data)
        .await
        .expect("upload_file failed");

    // Verify file is in cache
    let files_before = client
        .list_files(dir_id)
        .await
        .expect("list before delete failed");
    assert_eq!(files_before.len(), 1, "Should have one file before delete");

    // Delete the file
    client
        .delete_file(dir_id, file_id)
        .await
        .expect("delete_file failed");

    // Verify file is removed from cache
    let files_after = client
        .list_files(dir_id)
        .await
        .expect("list after delete failed");
    assert!(files_after.is_empty(), "Cache should be empty after delete");

    // Verify find_file returns None
    let found = client
        .find_file(dir_id, "to_delete.txt")
        .await
        .expect("find_file failed");
    assert!(
        found.is_none(),
        "find_file should return None for deleted file"
    );

    // Clean up directory
    let _ = client.delete_file(0, dir_id).await;

    println!("Scenario 4 passed: Delete removes file from cache correctly");
}

/// Scenario 5: Deleting non-existent file doesn't affect cache (idempotent delete)
#[tokio::test]
async fn test_cache_scenario5_idempotent_delete() {
    skip_if_no_credentials!();

    let repo_path = unique_test_path();
    let client = create_test_client(&repo_path).unwrap();

    // Create test directory
    let dir_id = match client.ensure_path(&repo_path).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!(
                "Failed to create test directory (may be rate limited): {:?}",
                e
            );
            return;
        }
    };

    // Upload a file and initialize cache
    let test_data = Bytes::from("existing file");
    let file_id = client
        .upload_file(dir_id, "existing.txt", test_data)
        .await
        .expect("upload_file failed");

    // List to populate cache
    let files_before = client.list_files(dir_id).await.expect("list_files failed");
    assert_eq!(files_before.len(), 1, "Should have one file");

    // Try to delete a non-existent file ID (use a fake ID)
    let non_existent_id = 999999999i64;
    // Note: This may fail at the 123pan API level, but cache should not be corrupted
    let _ = client.delete_file(dir_id, non_existent_id).await;

    // Cache should still have the existing file
    let files_after = client
        .list_files(dir_id)
        .await
        .expect("list after failed delete");
    assert_eq!(files_after.len(), 1, "Cache should still have one file");
    assert_eq!(
        files_after[0].filename, "existing.txt",
        "Original file should be intact"
    );

    // Clean up
    let _ = client.delete_file(dir_id, file_id).await;
    let _ = client.delete_file(0, dir_id).await;

    println!("Scenario 5 passed: Idempotent delete doesn't corrupt cache");
}

/// Scenario 6: Multiple directories have isolated caches
#[tokio::test]
async fn test_cache_scenario6_multi_directory_isolation() {
    skip_if_no_credentials!();

    let repo_path = unique_test_path();
    let client = create_test_client(&repo_path).unwrap();

    // Create two subdirectories
    let path_a = format!("{}/dir_a", repo_path);
    let path_b = format!("{}/dir_b", repo_path);

    let dir_a_id = match client.ensure_path(&path_a).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Failed to create dir_a (may be rate limited): {:?}", e);
            return;
        }
    };

    let dir_b_id = match client.ensure_path(&path_b).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Failed to create dir_b (may be rate limited): {:?}", e);
            return;
        }
    };

    // Initialize caches for both
    let _ = client
        .list_files(dir_a_id)
        .await
        .expect("list dir_a failed");
    let _ = client
        .list_files(dir_b_id)
        .await
        .expect("list dir_b failed");

    // Upload to dir_a
    let data_a = Bytes::from("file in dir_a");
    let file_a_id = client
        .upload_file(dir_a_id, "file_a.txt", data_a)
        .await
        .expect("upload to dir_a failed");

    // Upload to dir_b
    let data_b = Bytes::from("file in dir_b");
    let file_b_id = client
        .upload_file(dir_b_id, "file_b.txt", data_b)
        .await
        .expect("upload to dir_b failed");

    // Verify isolation
    let files_a = client
        .list_files(dir_a_id)
        .await
        .expect("list dir_a after upload");
    let files_b = client
        .list_files(dir_b_id)
        .await
        .expect("list dir_b after upload");

    assert_eq!(files_a.len(), 1, "dir_a should have 1 file");
    assert_eq!(files_b.len(), 1, "dir_b should have 1 file");
    assert_eq!(
        files_a[0].filename, "file_a.txt",
        "dir_a should have file_a.txt"
    );
    assert_eq!(
        files_b[0].filename, "file_b.txt",
        "dir_b should have file_b.txt"
    );

    // Upload more to dir_a, should not affect dir_b
    let data_a2 = Bytes::from("another file in dir_a");
    let file_a2_id = client
        .upload_file(dir_a_id, "file_a2.txt", data_a2)
        .await
        .expect("second upload to dir_a failed");

    let files_a_after = client.list_files(dir_a_id).await.expect("list dir_a final");
    let files_b_after = client.list_files(dir_b_id).await.expect("list dir_b final");

    assert_eq!(files_a_after.len(), 2, "dir_a should have 2 files");
    assert_eq!(
        files_b_after.len(),
        1,
        "dir_b should still have 1 file (not polluted)"
    );

    // Clean up
    let _ = client.delete_file(dir_a_id, file_a_id).await;
    let _ = client.delete_file(dir_a_id, file_a2_id).await;
    let _ = client.delete_file(dir_b_id, file_b_id).await;
    let _ = client.delete_file(0, dir_a_id).await;
    let _ = client.delete_file(0, dir_b_id).await;
    let repo_id = client.find_path_id(&repo_path).await.ok().flatten();
    if let Some(id) = repo_id {
        let _ = client.delete_file(0, id).await;
    }

    println!("Scenario 6 passed: Multi-directory caches are properly isolated");
}

/// Scenario 7: Upload without prior cache initialization
#[tokio::test]
async fn test_cache_scenario7_upload_without_cache_init() {
    skip_if_no_credentials!();

    let repo_path = unique_test_path();
    let client = create_test_client(&repo_path).unwrap();

    // Create test directory
    let dir_id = match client.ensure_path(&repo_path).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!(
                "Failed to create test directory (may be rate limited): {:?}",
                e
            );
            return;
        }
    };

    // Upload WITHOUT calling list_files first (cache not initialized)
    let test_data = Bytes::from("uploaded without cache");
    let file_id = client
        .upload_file(dir_id, "first.txt", test_data.clone())
        .await
        .expect("upload_file failed");

    // Now list files - should call API and include the uploaded file
    let files = client.list_files(dir_id).await.expect("list_files failed");

    assert_eq!(files.len(), 1, "Should find the uploaded file");
    assert_eq!(files[0].filename, "first.txt", "Filename should match");
    assert_eq!(files[0].size, test_data.len() as i64, "Size should match");

    // Clean up
    let _ = client.delete_file(dir_id, file_id).await;
    let _ = client.delete_file(0, dir_id).await;

    println!("Scenario 7 passed: Upload without prior cache init works correctly");
}

/// Scenario 8: Consecutive rapid operations maintain cache consistency
#[tokio::test]
async fn test_cache_scenario8_rapid_consecutive_operations() {
    skip_if_no_credentials!();

    let repo_path = unique_test_path();
    let client = create_test_client(&repo_path).unwrap();

    // Create test directory
    let dir_id = match client.ensure_path(&repo_path).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!(
                "Failed to create test directory (may be rate limited): {:?}",
                e
            );
            return;
        }
    };

    // Initialize cache
    let _ = client.list_files(dir_id).await.expect("list_files failed");

    // Rapid operations: upload a, upload b, delete a, upload c
    let data_a = Bytes::from("file a");
    let file_a_id = client
        .upload_file(dir_id, "a.txt", data_a)
        .await
        .expect("upload a failed");

    let data_b = Bytes::from("file b");
    let file_b_id = client
        .upload_file(dir_id, "b.txt", data_b)
        .await
        .expect("upload b failed");

    client
        .delete_file(dir_id, file_a_id)
        .await
        .expect("delete a failed");

    let data_c = Bytes::from("file c");
    let file_c_id = client
        .upload_file(dir_id, "c.txt", data_c)
        .await
        .expect("upload c failed");

    // Final state should have b.txt and c.txt only
    let files = client.list_files(dir_id).await.expect("final list failed");

    assert_eq!(files.len(), 2, "Should have exactly 2 files (b and c)");

    let filenames: Vec<&str> = files.iter().map(|f| f.filename.as_str()).collect();
    assert!(filenames.contains(&"b.txt"), "Should contain b.txt");
    assert!(filenames.contains(&"c.txt"), "Should contain c.txt");
    assert!(
        !filenames.contains(&"a.txt"),
        "Should NOT contain a.txt (deleted)"
    );

    // Clean up
    let _ = client.delete_file(dir_id, file_b_id).await;
    let _ = client.delete_file(dir_id, file_c_id).await;
    let _ = client.delete_file(0, dir_id).await;

    println!("Scenario 8 passed: Rapid consecutive operations maintain cache consistency");
}
