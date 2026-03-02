use std::time::Duration;

pub const MAX_RETRIES: usize = 3;
pub const RETRY_DELAY: Duration = Duration::from_secs(1);

pub mod auth;
pub mod client;
pub mod entity;
pub mod types;

#[cfg(test)]
mod tests;

pub use client::Pan123Client;
pub use types::{
    ApiResponse, CreateDirData, CreateDirRequest, DeleteRequest, DownloadInfoData, FileInfo,
    FileListData, MoveRequest, S3AuthData, SignInData, SignInRequest, SignInResponse,
    SingleUploadData, TrashRequest, UploadCompleteV2Data, UploadRequestData,
};
