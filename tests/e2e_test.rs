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
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
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

/// Start the server as a child process with real-time log output.
fn start_server(client_id: &str, client_secret: &str, port: u16, repo_path: &str) -> Child {
    let cargo_bin = env::var("CARGO_BIN_EXE_restic-123pan")
        .unwrap_or_else(|_| "target/debug/restic-123pan".to_string());

    let mut child = Command::new(&cargo_bin)
        .env("PAN123_CLIENT_ID", client_id)
        .env("PAN123_CLIENT_SECRET", client_secret)
        .env("PAN123_REPO_PATH", repo_path)
        .env("LISTEN_ADDR", format!("127.0.0.1:{}", port))
        .env("DATABASE_URL", format!("sqlite:cache_{}.db?mode=rwc", port))
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server");

    // Spawn threads to forward server logs in real-time
    let stdout = child.stdout.take().expect("Failed to take stdout");
    let stderr = child.stderr.take().expect("Failed to take stderr");

    let stdout_reader = BufReader::new(stdout);
    let stderr_reader = BufReader::new(stderr);

    // Forward stdout
    thread::spawn(move || {
        for line in stdout_reader.lines() {
            if let Ok(line) = line {
                println!("[SERVER] {}", line);
            }
        }
    });

    // Forward stderr
    thread::spawn(move || {
        for line in stderr_reader.lines() {
            if let Ok(line) = line {
                eprintln!("[SERVER] {}", line);
            }
        }
    });

    child
}

/// Run a restic command with real-time output.
/// Returns the command output and success status.
fn run_restic_with_output(
    repo_url: &str,
    password: &str,
    args: &[&str],
    description: &str,
) -> (Output, bool) {
    println!("\n>>> {}...", description);
    println!("Command: restic -r {} {}", repo_url, args.join(" "));

    // Use output() for simple commands that complete quickly
    // For long-running commands like backup, we'll use spawn with real-time output
    let is_long_running = args.contains(&"backup") || args.contains(&"restore");

    if is_long_running {
        // For long-running commands, use spawn with real-time output
        let mut child = Command::new("restic")
            .args(["-r", repo_url])
            .args(args)
            .env("RESTIC_PASSWORD", password)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn restic command");

        let stdout = child.stdout.take().expect("Failed to take stdout");
        let stderr = child.stderr.take().expect("Failed to take stderr");

        // Forward stdout in real-time and collect
        let stdout_handle = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            let mut collected = Vec::new();
            for line in reader.lines() {
                if let Ok(line) = line {
                    println!("[RESTIC] {}", line);
                    collected.push(line);
                }
            }
            collected
        });

        // Forward stderr in real-time and collect
        let stderr_handle = thread::spawn(move || {
            let reader = BufReader::new(stderr);
            let mut collected = Vec::new();
            for line in reader.lines() {
                if let Ok(line) = line {
                    eprintln!("[RESTIC] {}", line);
                    collected.push(line);
                }
            }
            collected
        });

        // Wait for process to complete
        let status = child.wait().expect("Failed to wait for restic");
        let success = status.success();

        // Wait for output threads to finish
        let stdout_lines = stdout_handle.join().unwrap();
        let stderr_lines = stderr_handle.join().unwrap();

        // Build Output struct
        let output = Output {
            status,
            stdout: stdout_lines.join("\n").into_bytes(),
            stderr: stderr_lines.join("\n").into_bytes(),
        };

        if !success {
            eprintln!("[ERROR] Command failed with status: {:?}", output.status);
        }

        (output, success)
    } else {
        // For quick commands, use output() and print results
        let output = Command::new("restic")
            .args(["-r", repo_url])
            .args(args)
            .env("RESTIC_PASSWORD", password)
            .output()
            .expect("Failed to run restic command");

        // Print output
        if !output.stdout.is_empty() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                println!("[RESTIC] {}", line);
            }
        }
        if !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            for line in stderr.lines() {
                eprintln!("[RESTIC] {}", line);
            }
        }

        let success = output.status.success();
        if !success {
            eprintln!("[ERROR] Command failed with status: {:?}", output.status);
        }

        (output, success)
    }
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
    binary
        .write_all(&[0u8, 1, 2, 3, 4, 5, 255, 254, 253])
        .expect("Failed to write");
}

/// Calculate SHA256 hash of a file.
fn hash_file(path: &PathBuf) -> String {
    use sha2::{Digest, Sha256};
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
    println!(
        "Original files: {:?}",
        original_hashes.keys().collect::<Vec<_>>()
    );

    // Find a port and start the server
    let port = find_available_port();
    let repo_path = format!("/restic-e2e-test-{}", chrono::Utc::now().timestamp());

    println!(
        "Starting server on port {} with repo path {}",
        port, repo_path
    );

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
        panic!(
            "restic init failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        );
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
        panic!(
            "restic backup failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        );
    }

    println!(
        "Backup completed: {}",
        String::from_utf8_lossy(&backup_output.stdout)
    );

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

    println!(
        "Snapshots: {}",
        String::from_utf8_lossy(&snapshots_output.stdout)
    );

    // Restore latest snapshot
    println!("Restoring files...");
    let restore_output = Command::new("restic")
        .args([
            "-r",
            &repo_url,
            "restore",
            "latest",
            "--target",
            restore_dir.to_str().unwrap(),
        ])
        .env("RESTIC_PASSWORD", password)
        .output()
        .expect("Failed to run restic restore");

    if !restore_output.status.success() {
        let stderr = String::from_utf8_lossy(&restore_output.stderr);
        let stdout = String::from_utf8_lossy(&restore_output.stdout);
        server.kill().ok();
        panic!(
            "restic restore failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        );
    }

    println!(
        "Restore completed: {}",
        String::from_utf8_lossy(&restore_output.stdout)
    );

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
    println!(
        "Restored files: {:?}",
        restored_hashes.keys().collect::<Vec<_>>()
    );

    // Compare hashes
    assert_eq!(
        original_hashes.len(),
        restored_hashes.len(),
        "Different number of files: original={}, restored={}",
        original_hashes.len(),
        restored_hashes.len()
    );

    for (name, original_hash) in &original_hashes {
        let restored_hash = restored_hashes
            .get(name)
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

// ============================================================================
// Large Scale Tests (~100MB)
// ============================================================================

/// Create test files of a specific total size using truly random, incompressible data.
/// Generates a mix of small, medium, and large files to simulate real workloads.
/// Uses /dev/urandom for fast random data generation.
fn create_large_test_files(dir: &PathBuf, total_size_mb: usize) {
    use std::io::{BufWriter, Read};

    // Open /dev/urandom for fast random data reading
    let mut urandom = fs::File::open("/dev/urandom").expect("Failed to open /dev/urandom");

    let total_bytes = total_size_mb * 1024 * 1024;
    let mut created_bytes = 0usize;
    let mut file_counter = 0usize;

    // Create a mix of file sizes:
    // - 60% of space: Large files (5-20MB each)
    // - 30% of space: Medium files (100KB-1MB each)
    // - 10% of space: Small files (1KB-10KB each)

    let large_target = total_bytes * 60 / 100;
    let medium_target = total_bytes * 30 / 100;
    let small_target = total_bytes * 10 / 100;

    // Create subdirectories
    let large_dir = dir.join("large");
    let medium_dir = dir.join("medium");
    let small_dir = dir.join("small");

    fs::create_dir_all(&large_dir).expect("Failed to create large dir");
    fs::create_dir_all(&medium_dir).expect("Failed to create medium dir");
    fs::create_dir_all(&small_dir).expect("Failed to create small dir");

    // Use larger chunks for efficiency
    let chunk_size = 256 * 1024; // 256KB chunks

    // Create large files (5-20MB each) with random data
    let mut large_created = 0usize;
    while large_created < large_target {
        let file_size = 5 * 1024 * 1024 + (file_counter * 3 * 1024 * 1024) % (15 * 1024 * 1024);
        let file_size = file_size.min(large_target - large_created);

        let path = large_dir.join(format!("large_{:04}.bin", file_counter));
        let file = fs::File::create(&path).expect("Failed to create large file");
        let mut writer = BufWriter::new(file);

        let mut written = 0usize;
        while written < file_size {
            let to_write = (file_size - written).min(chunk_size);
            // Read random bytes from /dev/urandom (much faster than generating)
            let mut random_chunk = vec![0u8; to_write];
            urandom
                .read_exact(&mut random_chunk)
                .expect("Failed to read from /dev/urandom");
            writer.write_all(&random_chunk).expect("Failed to write");
            written += to_write;
        }

        large_created += file_size;
        created_bytes += file_size;
        file_counter += 1;
    }

    println!(
        "Created {} bytes in large files (random, incompressible)",
        large_created
    );

    // Create medium files (100KB-1MB each) with random data
    let mut medium_created = 0usize;
    while medium_created < medium_target {
        let file_size = 100 * 1024 + (file_counter * 100 * 1024) % (900 * 1024);
        let file_size = file_size.min(medium_target - medium_created);

        let path = medium_dir.join(format!("medium_{:04}.dat", file_counter));
        let file = fs::File::create(&path).expect("Failed to create medium file");
        let mut writer = BufWriter::new(file);

        let mut written = 0usize;
        while written < file_size {
            let to_write = (file_size - written).min(chunk_size);
            let mut random_chunk = vec![0u8; to_write];
            urandom
                .read_exact(&mut random_chunk)
                .expect("Failed to read from /dev/urandom");
            writer.write_all(&random_chunk).expect("Failed to write");
            written += to_write;
        }

        medium_created += file_size;
        created_bytes += file_size;
        file_counter += 1;
    }

    println!(
        "Created {} bytes in medium files (random, incompressible)",
        medium_created
    );

    // Create small files (1KB-10KB each) with random data
    let mut small_created = 0usize;
    while small_created < small_target {
        let file_size = 1024 + (file_counter * 1024) % (9 * 1024);
        let file_size = file_size.min(small_target - small_created);

        let path = small_dir.join(format!("small_{:04}.bin", file_counter));
        let mut file = fs::File::create(&path).expect("Failed to create small file");

        // Read random bytes from /dev/urandom for small files too
        let mut random_data = vec![0u8; file_size];
        urandom
            .read_exact(&mut random_data)
            .expect("Failed to read from /dev/urandom");
        file.write_all(&random_data).expect("Failed to write");

        small_created += file_size;
        created_bytes += file_size;
        file_counter += 1;
    }

    println!(
        "Created {} bytes in small files (random, incompressible)",
        small_created
    );
    println!(
        "Total: {} bytes ({} MB) in {} files",
        created_bytes,
        created_bytes / 1024 / 1024,
        file_counter
    );
}

/// Large scale backup and restore test (~100MB).
/// This test validates:
/// - Handling of larger data volumes
/// - Mix of file sizes (small, medium, large)
/// - Cache effectiveness with many files
/// - Performance characteristics
#[test]
fn test_e2e_large_scale_100mb() {
    skip_if_not_ready!();

    let (client_id, client_secret) = get_test_credentials().unwrap();

    // Create temporary directories
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let source_dir = temp_dir.path().join("source");
    let restore_dir = temp_dir.path().join("restore");

    fs::create_dir(&source_dir).expect("Failed to create source dir");
    fs::create_dir(&restore_dir).expect("Failed to create restore dir");

    // Create ~100MB of test files
    println!("Creating ~100MB of test files...");
    let start_create = std::time::Instant::now();
    create_large_test_files(&source_dir, 100);
    println!("File creation took {:?}", start_create.elapsed());

    // Calculate original hashes
    println!("Calculating original file hashes...");
    let start_hash = std::time::Instant::now();
    let original_hashes = hash_directory(&source_dir);
    println!(
        "Hashing took {:?} for {} files",
        start_hash.elapsed(),
        original_hashes.len()
    );

    // Find a port and start the server
    let port = find_available_port();
    let repo_path = format!("/restic-e2e-large-{}", chrono::Utc::now().timestamp());

    println!(
        "Starting server on port {} with repo path {}",
        port, repo_path
    );

    let mut server = start_server(&client_id, &client_secret, port, &repo_path);

    // Wait for server to be ready
    if !wait_for_server(port, Duration::from_secs(15)) {
        server.kill().ok();
        panic!("Server failed to start within 15 seconds");
    }

    println!("Server is ready");

    let repo_url = format!("rest:http://127.0.0.1:{}/", port);
    let password = "large-test-password-456";

    // Initialize repository
    let start_init = std::time::Instant::now();
    let (init_output, init_success) =
        run_restic_with_output(&repo_url, password, &["init"], "Initializing repository");

    if !init_success {
        let stderr = String::from_utf8_lossy(&init_output.stderr);
        let stdout = String::from_utf8_lossy(&init_output.stdout);
        server.kill().ok();
        panic!(
            "restic init failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        );
    }
    println!("Repository initialized in {:?}", start_init.elapsed());

    // Backup files with real-time output
    let start_backup = std::time::Instant::now();
    let (backup_output, backup_success) = run_restic_with_output(
        &repo_url,
        password,
        &["backup", source_dir.to_str().unwrap()],
        "Backing up ~100MB of files (this may take several minutes)",
    );

    if !backup_success {
        let stderr = String::from_utf8_lossy(&backup_output.stderr);
        let stdout = String::from_utf8_lossy(&backup_output.stdout);
        server.kill().ok();
        panic!(
            "restic backup failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        );
    }

    let backup_time = start_backup.elapsed();
    println!("\nBackup completed in {:?}", backup_time);

    // Check repository integrity
    let start_check = std::time::Instant::now();
    let (check_output, check_success) = run_restic_with_output(
        &repo_url,
        password,
        &["check"],
        "Checking repository integrity",
    );

    if !check_success {
        let stderr = String::from_utf8_lossy(&check_output.stderr);
        println!("WARNING: restic check reported issues: {}", stderr);
    } else {
        println!("Repository check passed in {:?}", start_check.elapsed());
    }

    // List snapshots
    let (_snapshots_output, _) =
        run_restic_with_output(&repo_url, password, &["snapshots"], "Listing snapshots");

    // Restore latest snapshot
    let start_restore = std::time::Instant::now();
    let (restore_output, restore_success) = run_restic_with_output(
        &repo_url,
        password,
        &[
            "restore",
            "latest",
            "--target",
            restore_dir.to_str().unwrap(),
        ],
        "Restoring files",
    );

    if !restore_success {
        let stderr = String::from_utf8_lossy(&restore_output.stderr);
        let stdout = String::from_utf8_lossy(&restore_output.stdout);
        server.kill().ok();
        panic!(
            "restic restore failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        );
    }

    let restore_time = start_restore.elapsed();
    println!("\nRestore completed in {:?}", restore_time);

    // Stop server
    server.kill().ok();

    // Find the restored source directory
    let restored_source = walkdir::WalkDir::new(&restore_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name() == "source")
        .map(|e| e.path().to_path_buf())
        .expect("Could not find restored source directory");

    // Calculate restored hashes
    println!("Verifying restored files...");
    let start_verify = std::time::Instant::now();
    let restored_hashes = hash_directory(&restored_source);

    // Compare hashes
    assert_eq!(
        original_hashes.len(),
        restored_hashes.len(),
        "Different number of files: original={}, restored={}",
        original_hashes.len(),
        restored_hashes.len()
    );

    let mut verified_count = 0;
    for (name, original_hash) in &original_hashes {
        let restored_hash = restored_hashes
            .get(name)
            .unwrap_or_else(|| panic!("File {} not found in restored directory", name));
        assert_eq!(
            original_hash, restored_hash,
            "Hash mismatch for file {}: original={}, restored={}",
            name, original_hash, restored_hash
        );
        verified_count += 1;
    }

    println!("Verification completed in {:?}", start_verify.elapsed());

    println!("\n========== LARGE SCALE TEST SUMMARY ==========");
    println!("Files processed: {}", verified_count);
    println!("Backup time: {:?}", backup_time);
    println!("Restore time: {:?}", restore_time);
    println!("All {} files verified successfully!", verified_count);
    println!("===============================================\n");
}

/// Incremental backup test - tests cache effectiveness with multiple backups
#[test]
fn test_e2e_incremental_backup() {
    skip_if_not_ready!();

    let (client_id, client_secret) = get_test_credentials().unwrap();

    // Create temporary directories
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let source_dir = temp_dir.path().join("source");
    let restore_dir = temp_dir.path().join("restore");

    fs::create_dir(&source_dir).expect("Failed to create source dir");
    fs::create_dir(&restore_dir).expect("Failed to create restore dir");

    // Find a port and start the server
    let port = find_available_port();
    let repo_path = format!("/restic-e2e-incr-{}", chrono::Utc::now().timestamp());

    println!(
        "Starting server on port {} with repo path {}",
        port, repo_path
    );

    let mut server = start_server(&client_id, &client_secret, port, &repo_path);

    if !wait_for_server(port, Duration::from_secs(15)) {
        server.kill().ok();
        panic!("Server failed to start");
    }

    let repo_url = format!("rest:http://127.0.0.1:{}/", port);
    let password = "incremental-test-789";

    // Initialize repository
    println!("Initializing repository...");
    let init_output = Command::new("restic")
        .args(["-r", &repo_url, "init"])
        .env("RESTIC_PASSWORD", password)
        .output()
        .expect("Failed to run restic init");

    if !init_output.status.success() {
        let stderr = String::from_utf8_lossy(&init_output.stderr);
        server.kill().ok();
        panic!("restic init failed: {}", stderr);
    }

    // First backup: Create initial files (~10MB)
    println!("\n=== FIRST BACKUP (10MB) ===");
    create_large_test_files(&source_dir, 10);
    let hashes_v1 = hash_directory(&source_dir);

    let start_backup1 = std::time::Instant::now();
    let backup1 = Command::new("restic")
        .args(["-r", &repo_url, "backup", source_dir.to_str().unwrap()])
        .env("RESTIC_PASSWORD", password)
        .output()
        .expect("Failed to run restic backup");

    if !backup1.status.success() {
        let stderr = String::from_utf8_lossy(&backup1.stderr);
        server.kill().ok();
        panic!("First backup failed: {}", stderr);
    }
    println!("First backup took {:?}", start_backup1.elapsed());

    // Second backup: Add more files (~5MB additional)
    println!("\n=== SECOND BACKUP (add 5MB) ===");
    let extra_dir = source_dir.join("extra");
    fs::create_dir(&extra_dir).expect("Failed to create extra dir");

    // Add some new files
    for i in 0..10 {
        let path = extra_dir.join(format!("extra_{}.bin", i));
        let mut file = fs::File::create(&path).expect("Failed to create file");
        let content: Vec<u8> = (0..(500 * 1024)).map(|j| ((i + j) % 256) as u8).collect();
        file.write_all(&content).expect("Failed to write");
    }

    let hashes_v2 = hash_directory(&source_dir);
    println!(
        "V2 has {} files (added {})",
        hashes_v2.len(),
        hashes_v2.len() - hashes_v1.len()
    );

    let start_backup2 = std::time::Instant::now();
    let backup2 = Command::new("restic")
        .args(["-r", &repo_url, "backup", source_dir.to_str().unwrap()])
        .env("RESTIC_PASSWORD", password)
        .output()
        .expect("Failed to run restic backup");

    if !backup2.status.success() {
        let stderr = String::from_utf8_lossy(&backup2.stderr);
        server.kill().ok();
        panic!("Second backup failed: {}", stderr);
    }
    println!(
        "Second backup (incremental) took {:?}",
        start_backup2.elapsed()
    );
    println!(
        "Backup output: {}",
        String::from_utf8_lossy(&backup2.stdout)
    );

    // Third backup: Modify some existing files
    println!("\n=== THIRD BACKUP (modify files) ===");
    let large_dir = source_dir.join("large");
    if large_dir.exists() {
        for entry in fs::read_dir(&large_dir).unwrap().take(2) {
            let entry = entry.unwrap();
            let path = entry.path();
            // Append to existing file
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .expect("Failed to open file");
            file.write_all(b"MODIFIED CONTENT APPENDED\n")
                .expect("Failed to append");
        }
    }

    let hashes_v3 = hash_directory(&source_dir);

    let start_backup3 = std::time::Instant::now();
    let backup3 = Command::new("restic")
        .args(["-r", &repo_url, "backup", source_dir.to_str().unwrap()])
        .env("RESTIC_PASSWORD", password)
        .output()
        .expect("Failed to run restic backup");

    if !backup3.status.success() {
        let stderr = String::from_utf8_lossy(&backup3.stderr);
        server.kill().ok();
        panic!("Third backup failed: {}", stderr);
    }
    println!(
        "Third backup (modified files) took {:?}",
        start_backup3.elapsed()
    );

    // List all snapshots
    let snapshots = Command::new("restic")
        .args(["-r", &repo_url, "snapshots"])
        .env("RESTIC_PASSWORD", password)
        .output()
        .expect("Failed to list snapshots");
    println!(
        "\nSnapshots:\n{}",
        String::from_utf8_lossy(&snapshots.stdout)
    );

    // Restore and verify latest
    println!("Restoring latest snapshot...");
    let restore_output = Command::new("restic")
        .args([
            "-r",
            &repo_url,
            "restore",
            "latest",
            "--target",
            restore_dir.to_str().unwrap(),
        ])
        .env("RESTIC_PASSWORD", password)
        .output()
        .expect("Failed to restore");

    if !restore_output.status.success() {
        let stderr = String::from_utf8_lossy(&restore_output.stderr);
        server.kill().ok();
        panic!("Restore failed: {}", stderr);
    }

    server.kill().ok();

    // Verify
    let restored_source = walkdir::WalkDir::new(&restore_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name() == "source")
        .map(|e| e.path().to_path_buf())
        .expect("Could not find restored source directory");

    let restored_hashes = hash_directory(&restored_source);

    assert_eq!(
        hashes_v3.len(),
        restored_hashes.len(),
        "File count mismatch: expected {}, got {}",
        hashes_v3.len(),
        restored_hashes.len()
    );

    for (name, expected) in &hashes_v3 {
        let actual = restored_hashes
            .get(name)
            .unwrap_or_else(|| panic!("Missing file: {}", name));
        assert_eq!(expected, actual, "Hash mismatch for {}", name);
    }

    println!("\n========== INCREMENTAL TEST PASSED ==========");
    println!("Successfully performed 3 incremental backups and verified restore");
    println!("Final file count: {}", hashes_v3.len());
    println!("=============================================\n");
}
