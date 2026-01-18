//! Restic REST API v2 types.

use serde::{Deserialize, Serialize};

/// File entry for API v2 (name and size).
#[derive(Debug, Serialize, Deserialize)]
pub struct FileEntryV2 {
    pub name: String,
    pub size: u64,
}

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

    pub fn is_config(&self) -> bool {
        matches!(self, ResticFileType::Config)
    }
}

impl From<&crate::pan123::FileInfo> for FileEntryV2 {
    fn from(file: &crate::pan123::FileInfo) -> Self {
        Self {
            name: file.filename.clone(),
            size: file.size as u64,
        }
    }
}
