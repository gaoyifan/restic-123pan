//! 123pan API request and response types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

/// Request body for `sign_in`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignInRequest {
    pub passport: String,
    pub password: String,
    pub remember: bool,
}

/// Response data for `sign_in`.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SignInData {
    pub token: String,
    pub expire: String,
}

/// Dedicated response for sign-in (success code is 200, not 0).
#[derive(Debug, Deserialize, Clone)]
pub struct SignInResponse {
    pub code: i32,
    pub message: String,
    pub data: Option<SignInData>,
}

// ============================================================================
// File Operations
// ============================================================================

/// A file or folder in 123pan.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    #[serde(alias = "FileId")]
    pub file_id: i64,
    #[serde(alias = "FileName")]
    pub filename: String,
    #[serde(rename = "type")]
    #[serde(alias = "Type")]
    pub file_type: i32, // 0 = file, 1 = folder
    #[serde(alias = "Size")]
    pub size: i64,
    #[allow(dead_code)]
    #[serde(alias = "ParentFileId")]
    pub parent_file_id: i64,
    #[serde(default, deserialize_with = "deserialize_trashed")]
    pub trashed: i32, // 0 = not trashed, 1 = trashed
    #[serde(alias = "Etag", default)]
    pub etag: Option<String>,
    #[serde(alias = "S3KeyFlag", default)]
    pub s3_key_flag: Option<String>,
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

fn deserialize_trashed<'de, D>(deserializer: D) -> std::result::Result<i32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Trashed {
        Int(i32),
        Bool(bool),
    }

    let v = Option::<Trashed>::deserialize(deserializer)?;
    Ok(match v {
        Some(Trashed::Int(i)) => {
            if i == 0 {
                0
            } else {
                1
            }
        }
        Some(Trashed::Bool(b)) => {
            if b {
                1
            } else {
                0
            }
        }
        None => 0,
    })
}

impl From<crate::pan123::entity::Model> for FileInfo {
    fn from(model: crate::pan123::entity::Model) -> Self {
        Self {
            file_id: model.file_id,
            filename: model.name,
            file_type: if model.is_dir { 1 } else { 0 },
            size: model.size,
            parent_file_id: model.parent_id,
            trashed: 0,
            etag: model.etag,
            s3_key_flag: None,
        }
    }
}

/// Response data for file list.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileListData {
    #[serde(alias = "Next", default, deserialize_with = "deserialize_last_file_id")]
    pub last_file_id: i64,
    #[serde(alias = "InfoList", default)]
    pub file_list: Vec<FileInfo>,
}

fn deserialize_last_file_id<'de, D>(deserializer: D) -> std::result::Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NextId {
        Int(i64),
        String(String),
    }

    let v = Option::<NextId>::deserialize(deserializer)?;
    Ok(match v {
        Some(NextId::Int(i)) => i,
        Some(NextId::String(s)) => s.parse().unwrap_or(-1),
        None => -1,
    })
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

/// Response data for upload request API (legacy web client API).
#[derive(Debug, Deserialize, Clone)]
pub struct UploadRequestData {
    #[serde(rename = "Key", default)]
    pub key: String,
    #[serde(rename = "Bucket", default)]
    pub bucket: String,
    #[serde(rename = "FileId", default)]
    pub file_id: i64,
    #[serde(rename = "Reuse", default)]
    pub reuse: bool,
    #[serde(rename = "Info", default)]
    pub info: Option<FileInfo>,
    #[serde(rename = "UploadId", default)]
    pub upload_id: String,
    #[serde(rename = "StorageNode", default)]
    pub storage_node: String,
}

/// Response data for getting download URL.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInfoData {
    #[serde(alias = "DownloadUrl")]
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

/// Response data for s3 auth API.
#[derive(Debug, Deserialize)]
pub struct S3AuthData {
    #[serde(rename = "presignedUrls", default)]
    pub presigned_urls: HashMap<String, String>,
}

/// Response data for upload complete v2 API.
#[derive(Debug, Deserialize)]
pub struct UploadCompleteV2Data {
    #[serde(rename = "file_info")]
    pub file_info: FileInfo,
}
