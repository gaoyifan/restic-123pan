//! Migration tool to convert flat data directory to two-level hash structure.
//!
//! This tool migrates data files from:
//!   {repo_path}/data/{filename}
//! to:
//!   {repo_path}/data/{prefix}/{filename}
//!
//! where {prefix} is the first 2 characters of the filename (hex: 00-ff).
//!
//! Usage:
//!   migrate_data_structure --repo-path /path/to/repo [--dry-run]
//!
//! The migration is idempotent - running it multiple times is safe.

use clap::Parser;
use restic_123pan::pan123::{Pan123Client, FileInfo};
use std::collections::HashMap;
use tracing_subscriber::EnvFilter;

/// All possible 2-character hex prefixes (00-ff)
const HEX_PREFIXES: &[&str] = &[
    "00", "01", "02", "03", "04", "05", "06", "07", "08", "09", "0a", "0b", "0c", "0d", "0e", "0f",
    "10", "11", "12", "13", "14", "15", "16", "17", "18", "19", "1a", "1b", "1c", "1d", "1e", "1f",
    "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "2a", "2b", "2c", "2d", "2e", "2f",
    "30", "31", "32", "33", "34", "35", "36", "37", "38", "39", "3a", "3b", "3c", "3d", "3e", "3f",
    "40", "41", "42", "43", "44", "45", "46", "47", "48", "49", "4a", "4b", "4c", "4d", "4e", "4f",
    "50", "51", "52", "53", "54", "55", "56", "57", "58", "59", "5a", "5b", "5c", "5d", "5e", "5f",
    "60", "61", "62", "63", "64", "65", "66", "67", "68", "69", "6a", "6b", "6c", "6d", "6e", "6f",
    "70", "71", "72", "73", "74", "75", "76", "77", "78", "79", "7a", "7b", "7c", "7d", "7e", "7f",
    "80", "81", "82", "83", "84", "85", "86", "87", "88", "89", "8a", "8b", "8c", "8d", "8e", "8f",
    "90", "91", "92", "93", "94", "95", "96", "97", "98", "99", "9a", "9b", "9c", "9d", "9e", "9f",
    "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8", "a9", "aa", "ab", "ac", "ad", "ae", "af",
    "b0", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "ba", "bb", "bc", "bd", "be", "bf",
    "c0", "c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8", "c9", "ca", "cb", "cc", "cd", "ce", "cf",
    "d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7", "d8", "d9", "da", "db", "dc", "dd", "de", "df",
    "e0", "e1", "e2", "e3", "e4", "e5", "e6", "e7", "e8", "e9", "ea", "eb", "ec", "ed", "ee", "ef",
    "f0", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "fa", "fb", "fc", "fd", "fe", "ff",
];

#[derive(Parser, Debug)]
#[command(name = "migrate_data_structure")]
#[command(about = "Migrate data files to two-level hash directory structure")]
struct Args {
    /// 123pan client ID
    #[arg(long, env = "PAN123_CLIENT_ID")]
    client_id: String,

    /// 123pan client secret
    #[arg(long, env = "PAN123_CLIENT_SECRET")]
    client_secret: String,

    /// Repository path on 123pan
    #[arg(long, env = "PAN123_REPO_PATH")]
    repo_path: String,

    /// Dry run - show what would be done without making changes
    #[arg(long, default_value = "false")]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    tracing::info!("Starting data structure migration for: {}", args.repo_path);
    if args.dry_run {
        tracing::info!("DRY RUN mode - no changes will be made");
    }

    let client = Pan123Client::new(
        args.client_id.clone(),
        args.client_secret.clone(),
        args.repo_path.clone(),
    );

    // Get the data directory ID
    let data_path = format!("{}/data", args.repo_path);
    let data_dir_id = match client.find_path_id(&data_path).await? {
        Some(id) => id,
        None => {
            tracing::info!("No data directory found at {} - nothing to migrate", data_path);
            return Ok(());
        }
    };

    tracing::info!("Found data directory with ID: {}", data_dir_id);

    // List all items in the data directory
    let items = client.list_files(data_dir_id).await?;
    
    // Separate files (need migration) from subdirectories (already exist)
    let mut files_to_migrate: Vec<FileInfo> = Vec::new();
    let mut existing_subdirs: HashMap<String, i64> = HashMap::new();
    
    for item in items {
        if item.is_folder() {
            existing_subdirs.insert(item.filename.clone(), item.file_id);
            tracing::debug!("Found existing subdirectory: {}", item.filename);
        } else {
            // File in flat structure - needs migration
            files_to_migrate.push(item);
        }
    }

    tracing::info!(
        "Found {} files to migrate, {} subdirectories already exist",
        files_to_migrate.len(),
        existing_subdirs.len()
    );

    if files_to_migrate.is_empty() {
        tracing::info!("No files need migration!");
        return Ok(());
    }

    // Step 1: Create all missing subdirectories (00-ff)
    tracing::info!("Step 1: Creating missing subdirectories...");
    let mut created_count = 0;
    
    for prefix in HEX_PREFIXES {
        if !existing_subdirs.contains_key(*prefix) {
            if args.dry_run {
                tracing::info!("Would create subdirectory: {}", prefix);
                created_count += 1;
            } else {
                let subdir_path = format!("{}/data/{}", args.repo_path, prefix);
                match client.ensure_path(&subdir_path).await {
                    Ok(id) => {
                        existing_subdirs.insert(prefix.to_string(), id);
                        created_count += 1;
                        tracing::debug!("Created subdirectory: {} (id={})", prefix, id);
                    }
                    Err(e) => {
                        tracing::error!("Failed to create subdirectory {}: {:?}", prefix, e);
                    }
                }
            }
        }
    }
    
    tracing::info!("Created {} new subdirectories", created_count);

    // Step 2: Group files by their target subdirectory
    let mut files_by_prefix: HashMap<String, Vec<i64>> = HashMap::new();
    
    for file in &files_to_migrate {
        let prefix: String = file.filename.chars().take(2).collect();
        files_by_prefix
            .entry(prefix)
            .or_default()
            .push(file.file_id);
    }

    // Step 3: Batch move files to their subdirectories (max 100 per API call)
    tracing::info!("Step 2: Moving files to subdirectories...");
    let mut total_moved = 0;
    let mut total_failed = 0;

    for (prefix, file_ids) in files_by_prefix {
        let target_dir_id = match existing_subdirs.get(&prefix) {
            Some(&id) => id,
            None => {
                tracing::error!("Subdirectory {} not found, skipping {} files", prefix, file_ids.len());
                total_failed += file_ids.len();
                continue;
            }
        };

        // Process in batches of 100
        for chunk in file_ids.chunks(100) {
            if args.dry_run {
                tracing::info!(
                    "Would move {} files to data/{}/",
                    chunk.len(),
                    prefix
                );
                total_moved += chunk.len();
            } else {
                match client.move_files(chunk.to_vec(), target_dir_id).await {
                    Ok(()) => {
                        total_moved += chunk.len();
                        tracing::info!(
                            "Moved {} files to data/{}/",
                            chunk.len(),
                            prefix
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to move {} files to data/{}/: {:?}",
                            chunk.len(),
                            prefix,
                            e
                        );
                        total_failed += chunk.len();
                    }
                }
            }
        }
    }

    tracing::info!("=== Migration Complete ===");
    tracing::info!("Total files: {}", files_to_migrate.len());
    tracing::info!("Moved: {}", total_moved);
    tracing::info!("Failed: {}", total_failed);

    if total_failed > 0 {
        tracing::warn!("Some files failed to move. Run again to retry.");
    }

    Ok(())
}
