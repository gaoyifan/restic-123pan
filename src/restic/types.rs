//! Restic REST API v2 types.

use serde::{Deserialize, Serialize};

/// File entry for API v2 (name and size).
#[derive(Debug, Serialize, Deserialize)]
pub struct FileEntryV2 {
    pub name: String,
    pub size: u64,
}
