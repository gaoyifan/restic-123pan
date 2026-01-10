//! End-to-end tests using the actual restic CLI.
//!
//! These tests require:
//! - Environment variables: PAN123_CLIENT_ID, PAN123_CLIENT_SECRET
//! - restic CLI installed and available in PATH
//!
//! The tests will:
//! 1. Start the REST API server
//! 2. Create test files
//! 3. Initialize a restic repository
//! 4. Backup files
//! 5. Restore files
//! 6. Verify restored files match originals
//! 7. Clean up

use std::env;
use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

/// Get test credentials from environment.
fn get_test_credentials() -> Option<(String, String)> {
    let client_id = env::var("PAN123_CLIENT_ID").ok()?;
    let client_secret = env::var("PAN123_CLIENT_SECRET").ok()?;
    Some((client_id, client_secret))
}

/// Find an available port.
fn find_available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port");
    listener.local_addr().unwrap().port()
}

/// Start the server as a child process.
fn start_server(client_id: &str, client_secret: &str, port: u16, repo_path: &str) -> Child {
    let cargo_bin = env::var("CARGO_BIN_EXE_restic-api-server-123pan")
        .unwrap_or_else(|_| "target/debug/restic-api-server-123pan".to_string());
    
    Command::new(&cargo_bin)
        .env("PAN123_CLIENT_ID", client_id)
        .env("PAN123_CLIENT_SECRET", client_secret)
        .env("PAN123_REPO_PATH", repo_path)
        .env("LISTEN_ADDR", format!("127.0.0.1:{}", port))
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server")
}

/// Wait for the server to be ready.
fn wait_for_server(port: u16, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    let url = format!("http://127.0.0.1:{}/", port);
    
    while start.elapsed() < timeout {
        if let Ok(response) = reqwest::blocking::get(&url) {
            // Server is up if we get any response (even 4xx)
            if response.status().is_client_error() || response.status().is_success() {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Create test files in a directory.
fn create_test_files(dir: &PathBuf) {
    // Create some test files
    let mut file1 = fs::File::create(dir.join("test1.txt")).expect("Failed to create file");
    writeln!(file1, "This is test file 1").expect("Failed to write");
    
    let mut file2 = fs::File::create(dir.join("test2.txt")).expect("Failed to create file");
    writeln!(file2, "This is test file 2 with more content").expect("Failed to write");
    
    // Create a subdirectory with a file
    let subdir = dir.join("subdir");
    fs::create_dir(&subdir).expect("Failed to create subdir");
    let mut file3 = fs::File::create(subdir.join("test3.txt")).expect("Failed to create file");
    writeln!(file3, "This is test file 3 in a subdirectory").expect("Failed to write");
    
    // Create a binary file
    let mut binary = fs::File::create(dir.join("binary.bin")).expect("Failed to create file");
    binary.write_all(&[0u8, 1, 2, 3, 4, 5, 255, 254, 253]).expect("Failed to write");
}

/// Calculate SHA256 hash of a file.
fn hash_file(path: &PathBuf) -> String {
    use sha2::{Sha256, Digest};
    let content = fs::read(path).expect("Failed to read file");
    format!("{:x}", Sha256::digest(&content))
}

/// Get hashes of all files in a directory recursively.
fn hash_directory(dir: &PathBuf) -> std::collections::HashMap<String, String> {
    use walkdir::WalkDir;
    
    let mut hashes = std::collections::HashMap::new();
    
    for entry in WalkDir::new(dir) {
        let entry = entry.expect("Failed to read entry");
        if entry.file_type().is_file() {
            let relative = entry.path().strip_prefix(dir).unwrap();
            let hash = hash_file(&entry.path().to_path_buf());
            hashes.insert(relative.to_string_lossy().to_string(), hash);
        }
    }
    
    hashes
}

/// Skip test if prerequisites are not met.
macro_rules! skip_if_not_ready {
    () => {
        if get_test_credentials().is_none() {
            eprintln!("Skipping test: PAN123_CLIENT_ID and PAN123_CLIENT_SECRET not set");
            return;
        }
        
        // Check if restic is installed
        if Command::new("restic").arg("version").output().is_err() {
            eprintln!("Skipping test: restic CLI not found in PATH");
            return;
        }
    };
}

#[test]
fn test_e2e_backup_and_restore() {
    skip_if_not_ready!();
    
    let (client_id, client_secret) = get_test_credentials().unwrap();
    
    // Create temporary directories
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let source_dir = temp_dir.path().join("source");
    let restore_dir = temp_dir.path().join("restore");
    
    fs::create_dir(&source_dir).expect("Failed to create source dir");
    fs::create_dir(&restore_dir).expect("Failed to create restore dir");
    
    // Create test files
    create_test_files(&source_dir);
    
    // Calculate original hashes
    let original_hashes = hash_directory(&source_dir);
    println!("Original files: {:?}", original_hashes.keys().collect::<Vec<_>>());
    
    // Find a port and start the server
    let port = find_available_port();
    let repo_path = format!("/restic-e2e-test-{}", chrono::Utc::now().timestamp());
    
    println!("Starting server on port {} with repo path {}", port, repo_path);
    
    let mut server = start_server(&client_id, &client_secret, port, &repo_path);
    
    // Wait for server to be ready
    if !wait_for_server(port, Duration::from_secs(10)) {
        server.kill().ok();
        panic!("Server failed to start within 10 seconds");
    }
    
    println!("Server is ready");
    
    let repo_url = format!("rest:http://127.0.0.1:{}/", port);
    let password = "test-password-123";
    
    // Initialize repository
    println!("Initializing repository...");
    let init_output = Command::new("restic")
        .args(["-r", &repo_url, "init"])
        .env("RESTIC_PASSWORD", password)
        .output()
        .expect("Failed to run restic init");
    
    if !init_output.status.success() {
        let stderr = String::from_utf8_lossy(&init_output.stderr);
        let stdout = String::from_utf8_lossy(&init_output.stdout);
        server.kill().ok();
        panic!("restic init failed:\nstdout: {}\nstderr: {}", stdout, stderr);
    }
    
    println!("Repository initialized");
    
    // Backup files
    println!("Backing up files...");
    let backup_output = Command::new("restic")
        .args(["-r", &repo_url, "backup", source_dir.to_str().unwrap()])
        .env("RESTIC_PASSWORD", password)
        .output()
        .expect("Failed to run restic backup");
    
    if !backup_output.status.success() {
        let stderr = String::from_utf8_lossy(&backup_output.stderr);
        let stdout = String::from_utf8_lossy(&backup_output.stdout);
        server.kill().ok();
        panic!("restic backup failed:\nstdout: {}\nstderr: {}", stdout, stderr);
    }
    
    println!("Backup completed: {}", String::from_utf8_lossy(&backup_output.stdout));
    
    // List snapshots
    println!("Listing snapshots...");
    let snapshots_output = Command::new("restic")
        .args(["-r", &repo_url, "snapshots"])
        .env("RESTIC_PASSWORD", password)
        .output()
        .expect("Failed to run restic snapshots");
    
    if !snapshots_output.status.success() {
        let stderr = String::from_utf8_lossy(&snapshots_output.stderr);
        server.kill().ok();
        panic!("restic snapshots failed: {}", stderr);
    }
    
    println!("Snapshots: {}", String::from_utf8_lossy(&snapshots_output.stdout));
    
    // Restore latest snapshot
    println!("Restoring files...");
    let restore_output = Command::new("restic")
        .args(["-r", &repo_url, "restore", "latest", "--target", restore_dir.to_str().unwrap()])
        .env("RESTIC_PASSWORD", password)
        .output()
        .expect("Failed to run restic restore");
    
    if !restore_output.status.success() {
        let stderr = String::from_utf8_lossy(&restore_output.stderr);
        let stdout = String::from_utf8_lossy(&restore_output.stdout);
        server.kill().ok();
        panic!("restic restore failed:\nstdout: {}\nstderr: {}", stdout, stderr);
    }
    
    println!("Restore completed: {}", String::from_utf8_lossy(&restore_output.stdout));
    
    // Stop server
    server.kill().ok();
    
    // Find the restored source directory (restic restores with full path)
    let restored_source = walkdir::WalkDir::new(&restore_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name() == "source")
        .map(|e| e.path().to_path_buf())
        .expect("Could not find restored source directory");
    
    // Calculate restored hashes
    let restored_hashes = hash_directory(&restored_source);
    println!("Restored files: {:?}", restored_hashes.keys().collect::<Vec<_>>());
    
    // Compare hashes
    assert_eq!(
        original_hashes.len(),
        restored_hashes.len(),
        "Different number of files: original={}, restored={}",
        original_hashes.len(),
        restored_hashes.len()
    );
    
    for (name, original_hash) in &original_hashes {
        let restored_hash = restored_hashes.get(name)
            .unwrap_or_else(|| panic!("File {} not found in restored directory", name));
        assert_eq!(
            original_hash, restored_hash,
            "Hash mismatch for file {}: original={}, restored={}",
            name, original_hash, restored_hash
        );
    }
    
    println!("All files verified successfully!");
}

/// Test that server starts and responds to basic requests.
#[test]
fn test_server_startup() {
    skip_if_not_ready!();
    
    let (client_id, client_secret) = get_test_credentials().unwrap();
    
    let port = find_available_port();
    let repo_path = format!("/restic-test-startup-{}", chrono::Utc::now().timestamp());
    
    println!("Starting server on port {}", port);
    
    let mut server = start_server(&client_id, &client_secret, port, &repo_path);
    
    // Wait for server to be ready
    if !wait_for_server(port, Duration::from_secs(10)) {
        server.kill().ok();
        panic!("Server failed to start");
    }
    
    // Test HEAD on config (should return 404 for non-existent repo)
    let response = reqwest::blocking::Client::new()
        .head(&format!("http://127.0.0.1:{}/config", port))
        .send()
        .expect("Failed to send request");
    
    println!("HEAD /config status: {}", response.status());
    
    // Should be 404 since repo doesn't exist
    assert!(
        response.status().as_u16() == 404 || response.status().as_u16() == 502,
        "Expected 404 or 502, got {}",
        response.status()
    );
    
    server.kill().ok();
    println!("Server startup test passed");
}
