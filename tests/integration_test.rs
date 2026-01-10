//! Integration tests for 123pan API operations.
//!
//! These tests require the following environment variables:
//! - PAN123_CLIENT_ID
//! - PAN123_CLIENT_SECRET

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
    Some(body["data"]["accessToken"].as_str().expect("No access token").to_string())
}

/// Skip test if rate limited.
macro_rules! skip_if_rate_limited {
    ($token:expr) => {
        let Some(token) = $token else {
            eprintln!("Skipping test due to rate limiting");
            return;
        };
        token
    };
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
    assert!(body["data"]["accessToken"].is_string(), "No access token in response");
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

    let list_body: serde_json::Value = list_response.json().await.expect("Failed to parse response");
    
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
    assert!(!domains.is_empty(), "Should have at least one upload domain");
    
    let domain = domains[0].as_str().expect("domain should be a string");
    println!("Got upload domain: {}", domain);
    assert!(domain.starts_with("https://"), "Domain should start with https://");
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

    let domain_body: serde_json::Value = domain_response.json().await.expect("Failed to parse response");
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
    let mkdir_body: serde_json::Value = mkdir_response.json().await.expect("Failed to parse response");
    
    println!("Mkdir response: {:?}", mkdir_body);
    
    assert!(status.is_success(), "HTTP request failed: {}", status);
    assert_eq!(mkdir_body["code"], 0, "API error: {}", mkdir_body["message"]);
    
    let dir_id = mkdir_body["data"]["dirID"].as_i64().expect("No dirID in response");
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

    let trash_body: serde_json::Value = trash_response.json().await.expect("Failed to parse response");
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

    let delete_body: serde_json::Value = delete_response.json().await.expect("Failed to parse response");
    println!("Delete response: {:?}", delete_body);
    assert_eq!(delete_body["code"], 0, "Delete failed: {}", delete_body["message"]);
    
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
            ("searchData", "nonexistent-file-name-12345")
        ])
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = response.json().await.expect("Failed to parse response");
    
    println!("Search response: {:?}", body);
    
    assert_eq!(body["code"], 0, "API error: {}", body["message"]);
    
    // Should return empty list for nonexistent file
    let file_list = body["data"]["fileList"].as_array().expect("fileList should be array");
    assert!(file_list.is_empty(), "Should return empty list for nonexistent file");
    
    println!("Search mode test passed");
}
