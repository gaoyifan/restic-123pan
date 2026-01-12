//! 123pan API request and response types.

use serde::{Deserialize, Serialize};

/// Base API response wrapper from 123pan.
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub code: i32,
    pub message: String,
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    /// Check if the response indicates success.
    pub fn is_success(&self) -> bool {
        self.code == 0
    }
}

// ============================================================================
// Authentication
// ============================================================================

/// Request body for getting access token.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessTokenRequest {
    pub client_id: String,
    pub client_secret: String,
}

/// Response data for access token.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccessTokenData {
    pub access_token: String,
    pub expired_at: String,
}

// ============================================================================
// File Operations
// ============================================================================

/// A file or folder in 123pan.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub file_id: i64,
    pub filename: String,
    #[serde(rename = "type")]
    pub file_type: i32, // 0 = file, 1 = folder
    pub size: i64,
    #[allow(dead_code)]
    pub parent_file_id: i64,
    #[serde(default)]
    pub trashed: i32, // 0 = not trashed, 1 = trashed
}

impl FileInfo {
    /// Check if this is a folder.
    pub fn is_folder(&self) -> bool {
        self.file_type == 1
    }

    /// Check if this item is in trash.
    pub fn is_trashed(&self) -> bool {
        self.trashed == 1
    }
}

/// Response data for file list.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileListData {
    pub last_file_id: i64,
    pub file_list: Vec<FileInfo>,
}

/// Request body for creating a directory.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDirRequest {
    pub name: String,
    #[serde(rename = "parentID")]
    pub parent_id: i64,
}

/// Response data for creating a directory.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDirData {
    #[serde(rename = "dirID")]
    pub dir_id: i64,
}

/// Response data for getting download URL.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInfoData {
    pub download_url: String,
}

/// Request body for moving files to trash.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashRequest {
    #[serde(rename = "fileIDs")]
    pub file_ids: Vec<i64>,
}

/// Request body for permanently deleting files.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRequest {
    #[serde(rename = "fileIDs")]
    pub file_ids: Vec<i64>,
}

/// Request body for moving files to a different directory.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRequest {
    #[serde(rename = "fileIDs")]
    pub file_ids: Vec<i64>,
    #[serde(rename = "toParentFileID")]
    pub to_parent_file_id: i64,
}

/// Response data for single file upload.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SingleUploadData {
    #[serde(rename = "fileID")]
    pub file_id: i64,
    pub completed: bool,
}

// ============================================================================
// File Type Mapping
// ============================================================================

/// Restic file types mapped to directory names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResticFileType {
    Config,
    Data,
    Keys,
    Locks,
    Snapshots,
    Index,
}

impl ResticFileType {
    /// Get the directory name for this file type.
    pub fn dirname(&self) -> &'static str {
        match self {
            ResticFileType::Config => "config",
            ResticFileType::Data => "data",
            ResticFileType::Keys => "keys",
            ResticFileType::Locks => "locks",
            ResticFileType::Snapshots => "snapshots",
            ResticFileType::Index => "index",
        }
    }

    /// Parse file type from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "config" => Some(ResticFileType::Config),
            "data" => Some(ResticFileType::Data),
            "keys" => Some(ResticFileType::Keys),
            "locks" => Some(ResticFileType::Locks),
            "snapshots" => Some(ResticFileType::Snapshots),
            "index" => Some(ResticFileType::Index),
            _ => None,
        }
    }

    /// Check if this type is config (special handling - single file, not directory).
    pub fn is_config(&self) -> bool {
        matches!(self, ResticFileType::Config)
    }
}
