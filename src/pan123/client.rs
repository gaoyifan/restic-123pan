//! 123pan API client for file operations.

use bytes::Bytes;
use parking_lot::RwLock;
use reqwest::multipart::{Form, Part};
use std::collections::HashMap;
use std::sync::Arc;

use super::auth::{TokenManager, BASE_URL};
use super::types::*;
use crate::error::{AppError, Result};

/// Client for interacting with 123pan API.
#[derive(Clone)]
pub struct Pan123Client {
    token_manager: TokenManager,
    repo_path: String,
    /// Cache of directory IDs: path -> file_id
    dir_cache: Arc<RwLock<HashMap<String, i64>>>,
    /// Cache of file listings: parent_id -> Vec<FileInfo>
    files_cache: Arc<RwLock<HashMap<i64, Vec<FileInfo>>>>,
    /// Upload domain (fetched dynamically)
    upload_domain: Arc<RwLock<Option<String>>>,
}

impl Pan123Client {
    /// Create a new 123pan client.
    pub fn new(client_id: String, client_secret: String, repo_path: String) -> Self {
        Self {
            token_manager: TokenManager::new(client_id, client_secret),
            repo_path,
            dir_cache: Arc::new(RwLock::new(HashMap::new())),
            files_cache: Arc::new(RwLock::new(HashMap::new())),
            upload_domain: Arc::new(RwLock::new(None)),
        }
    }

    /// Make an authenticated GET request with 429 retry support.
    /// Retries up to 3 times with 1 second delay on 429 rate limit errors.
    async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<ApiResponse<T>> {
        const MAX_RETRIES: usize = 3;
        const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

        for attempt in 0..=MAX_RETRIES {
            let token = self.token_manager.get_token().await?;

            let response = self
                .token_manager
                .http_client()
                .get(url)
                .header("Authorization", format!("Bearer {}", token))
                .header("Platform", "open_platform")
                .send()
                .await?;

            let api_response: ApiResponse<T> = response.json().await?;

            // Check for 429 rate limit error
            if api_response.code == 429 {
                if attempt < MAX_RETRIES {
                    tracing::warn!(
                        "Rate limited (429), waiting {}s before retry (attempt {}/{})",
                        RETRY_DELAY.as_secs(),
                        attempt + 1,
                        MAX_RETRIES
                    );
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                } else {
                    tracing::error!(
                        "Rate limited (429) after {} retries, giving up",
                        MAX_RETRIES
                    );
                    return Err(AppError::Pan123Api {
                        code: api_response.code,
                        message: api_response.message,
                    });
                }
            }

            return Ok(api_response);
        }

        unreachable!()
    }

    /// Make an authenticated GET request without timeout.
    /// Used for file listing which can take a long time for large directories.
    /// Retries on 429 rate limit errors.
    async fn get_no_timeout<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<ApiResponse<T>> {
        const MAX_RETRIES: usize = 3;
        const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

        for attempt in 0..=MAX_RETRIES {
            let token = self.token_manager.get_token().await?;

            let response = self
                .token_manager
                .http_client()
                .get(url)
                .timeout(std::time::Duration::MAX) // No timeout
                .header("Authorization", format!("Bearer {}", token))
                .header("Platform", "open_platform")
                .send()
                .await?;

            let api_response: ApiResponse<T> = response.json().await?;

            // Check for 429 rate limit error
            if api_response.code == 429 {
                if attempt < MAX_RETRIES {
                    tracing::warn!(
                        "Rate limited (429), waiting {}s before retry (attempt {}/{})",
                        RETRY_DELAY.as_secs(),
                        attempt + 1,
                        MAX_RETRIES
                    );
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                } else {
                    tracing::error!(
                        "Rate limited (429) after {} retries, giving up",
                        MAX_RETRIES
                    );
                    return Err(AppError::Pan123Api {
                        code: api_response.code,
                        message: api_response.message,
                    });
                }
            }

            return Ok(api_response);
        }

        unreachable!()
    }

    /// Make an authenticated POST request with JSON body and 429 retry support.
    /// Retries up to 3 times with 1 second delay on 429 rate limit errors.
    async fn post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<ApiResponse<T>> {
        const MAX_RETRIES: usize = 3;
        const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

        // Serialize body once for reuse in retries
        let body_json = serde_json::to_string(body)?;

        for attempt in 0..=MAX_RETRIES {
            let token = self.token_manager.get_token().await?;

            let response = self
                .token_manager
                .http_client()
                .post(url)
                .header("Authorization", format!("Bearer {}", token))
                .header("Platform", "open_platform")
                .header("Content-Type", "application/json")
                .body(body_json.clone())
                .send()
                .await?;

            let api_response: ApiResponse<T> = response.json().await?;

            // Check for 429 rate limit error
            if api_response.code == 429 {
                if attempt < MAX_RETRIES {
                    tracing::warn!(
                        "Rate limited (429), waiting {}s before retry (attempt {}/{})",
                        RETRY_DELAY.as_secs(),
                        attempt + 1,
                        MAX_RETRIES
                    );
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                } else {
                    tracing::error!(
                        "Rate limited (429) after {} retries, giving up",
                        MAX_RETRIES
                    );
                    return Err(AppError::Pan123Api {
                        code: api_response.code,
                        message: api_response.message,
                    });
                }
            }

            return Ok(api_response);
        }

        unreachable!()
    }

    // ========================================================================
    // Upload Domain
    // ========================================================================

    /// Get upload domain, fetching dynamically if not cached.
    /// Includes 429 retry support.
    async fn get_upload_domain(&self) -> Result<String> {
        // Check cache first
        {
            let cache = self.upload_domain.read();
            if let Some(domain) = cache.as_ref() {
                return Ok(domain.clone());
            }
        }

        // Fetch from API with 429 retry support
        const MAX_RETRIES: usize = 3;
        const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(1);
        let url = format!("{}/upload/v2/file/domain", BASE_URL);

        for attempt in 0..=MAX_RETRIES {
            let token = self.token_manager.get_token().await?;

            let response = self
                .token_manager
                .http_client()
                .get(&url)
                .header("Authorization", format!("Bearer {}", token))
                .header("Platform", "open_platform")
                .send()
                .await?;

            let api_response: ApiResponse<Vec<String>> = response.json().await?;

            // Check for 429 rate limit error
            if api_response.code == 429 {
                if attempt < MAX_RETRIES {
                    tracing::warn!(
                        "Rate limited (429) when fetching upload domain, waiting {}s before retry (attempt {}/{})",
                        RETRY_DELAY.as_secs(),
                        attempt + 1,
                        MAX_RETRIES
                    );
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                } else {
                    tracing::error!(
                        "Rate limited (429) after {} retries when fetching upload domain, giving up",
                        MAX_RETRIES
                    );
                    return Err(AppError::Pan123Api {
                        code: api_response.code,
                        message: api_response.message,
                    });
                }
            }

            if !api_response.is_success() {
                return Err(AppError::Pan123Api {
                    code: api_response.code,
                    message: api_response.message,
                });
            }

            let domains = api_response
                .data
                .ok_or_else(|| AppError::Internal("No upload domain in response".to_string()))?;

            let domain = domains
                .into_iter()
                .next()
                .ok_or_else(|| AppError::Internal("Empty upload domain list".to_string()))?;

            tracing::info!("Fetched upload domain: {}", domain);

            // Cache the domain
            {
                let mut cache = self.upload_domain.write();
                *cache = Some(domain.clone());
            }

            return Ok(domain);
        }

        unreachable!()
    }

    // ========================================================================
    // Directory Operations
    // ========================================================================

    /// List files in a directory.
    /// Uses cache if available; otherwise fetches from API and caches the result.
    pub async fn list_files(&self, parent_id: i64) -> Result<Vec<FileInfo>> {
        // Check cache first
        {
            let cache = self.files_cache.read();
            if let Some(files) = cache.get(&parent_id) {
                tracing::debug!(
                    "Cache hit for parent_id {}: {} files",
                    parent_id,
                    files.len()
                );
                return Ok(files.clone());
            }
        }

        tracing::debug!("Cache miss for parent_id {}, fetching from API", parent_id);
        let files = self.fetch_files_from_api(parent_id).await?;

        // Store in cache
        {
            let mut cache = self.files_cache.write();
            cache.insert(parent_id, files.clone());
        }

        Ok(files)
    }

    /// Fetch files from 123pan API (internal, bypasses cache).
    /// Uses no timeout to handle large directories with hundreds of thousands of files.
    async fn fetch_files_from_api(&self, parent_id: i64) -> Result<Vec<FileInfo>> {
        let mut all_files = Vec::new();
        let mut last_file_id: Option<i64> = None;
        let mut page_count = 0;

        loop {
            let mut url = format!(
                "{}/api/v2/file/list?parentFileId={}&limit=100",
                BASE_URL, parent_id
            );

            if let Some(id) = last_file_id {
                url.push_str(&format!("&lastFileId={}", id));
            }

            let response: ApiResponse<FileListData> = self.get_no_timeout(&url).await?;

            if !response.is_success() {
                return Err(AppError::Pan123Api {
                    code: response.code,
                    message: response.message,
                });
            }

            if let Some(data) = response.data {
                // Filter out trashed files
                let files: Vec<_> = data
                    .file_list
                    .into_iter()
                    .filter(|f| !f.is_trashed())
                    .collect();

                all_files.extend(files);
                page_count += 1;

                // Log progress for large directories
                if page_count % 100 == 0 {
                    tracing::info!(
                        "Fetched {} files so far (parent_id={})",
                        all_files.len(),
                        parent_id
                    );
                }

                if data.last_file_id == -1 {
                    break;
                }
                last_file_id = Some(data.last_file_id);
            } else {
                break;
            }
        }

        Ok(all_files)
    }

    /// Invalidate the files cache for a specific directory.
    /// Call this when external changes may have occurred.
    pub fn invalidate_files_cache(&self, parent_id: i64) {
        let mut cache = self.files_cache.write();
        cache.remove(&parent_id);
        tracing::debug!("Invalidated files cache for parent_id {}", parent_id);
    }

    /// Find a file by exact name in a directory.
    /// Uses directory listing instead of search (search has index delay issues).
    pub async fn find_file(&self, parent_id: i64, name: &str) -> Result<Option<FileInfo>> {
        let files = self.list_files(parent_id).await?;
        Ok(files.into_iter().find(|f| f.filename == name))
    }

    /// Create a directory. Returns the directory ID.
    /// If the directory already exists, returns an error with code 5063.
    async fn create_directory(&self, parent_id: i64, name: &str) -> Result<i64> {
        tracing::debug!("Creating directory '{}' in parent {}", name, parent_id);

        let request = CreateDirRequest {
            name: name.to_string(),
            parent_id,
        };

        // mkdir uses BASE_URL, not upload domain
        let url = format!("{}/upload/v1/file/mkdir", BASE_URL);

        let response: ApiResponse<CreateDirData> = self.post(&url, &request).await?;

        if !response.is_success() {
            tracing::debug!(
                "mkdir failed: code={}, message='{}'",
                response.code,
                response.message
            );
            // Code 1: directory already exists (message: 该目录下已经有同名文件夹,无法进行创建)
            if response.code == 1 && response.message.contains("同名") {
                tracing::debug!(
                    "Directory '{}' already exists, looking up its ID via list_files",
                    name
                );
                // Invalidate cache first since it might be stale
                self.invalidate_files_cache(parent_id);
                // Search mode has index delay, use list_files instead
                let files = self.list_files(parent_id).await?;
                if let Some(file) = files
                    .into_iter()
                    .find(|f| f.filename == name && f.is_folder())
                {
                    tracing::debug!(
                        "Found directory '{}' with id {}",
                        file.filename,
                        file.file_id
                    );
                    return Ok(file.file_id);
                } else {
                    tracing::debug!(
                        "Directory '{}' not found in list for parent {}",
                        name,
                        parent_id
                    );
                }
            }
            return Err(AppError::Pan123Api {
                code: response.code,
                message: response.message,
            });
        }

        let data = response
            .data
            .ok_or_else(|| AppError::Internal("No data in mkdir response".to_string()))?;

        // Add newly created directory to cache if cache exists
        {
            let mut cache = self.files_cache.write();
            if let Some(files) = cache.get_mut(&parent_id) {
                let new_dir = FileInfo {
                    file_id: data.dir_id,
                    filename: name.to_string(),
                    file_type: 1, // 1 = folder
                    size: 0,
                    parent_file_id: parent_id,
                    trashed: 0,
                };
                files.push(new_dir);
                tracing::debug!(
                    "Added new directory '{}' to cache (id={})",
                    name,
                    data.dir_id
                );
            }
        }

        tracing::info!("Created directory '{}' with id {}", name, data.dir_id);
        Ok(data.dir_id)
    }

    /// Find directory ID for a path by traversing from root.
    /// Does NOT create directories if they don't exist.
    pub async fn find_path_id(&self, path: &str) -> Result<Option<i64>> {
        // Check cache first
        {
            let cache = self.dir_cache.read();
            if let Some(&id) = cache.get(path) {
                return Ok(Some(id));
            }
        }

        let parts: Vec<&str> = path
            .trim_start_matches('/')
            .trim_end_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        let mut current_id: i64 = 0; // Root directory
        let mut current_path = String::new();

        for part in parts {
            current_path.push('/');
            current_path.push_str(part);

            // Check cache for this path segment
            {
                let cache = self.dir_cache.read();
                if let Some(&id) = cache.get(&current_path) {
                    current_id = id;
                    continue;
                }
            }

            // Look for the directory in the current parent
            if let Some(file) = self.find_file(current_id, part).await? {
                if file.is_folder() {
                    current_id = file.file_id;
                    // Update cache
                    {
                        let mut cache = self.dir_cache.write();
                        cache.insert(current_path.clone(), current_id);
                    }
                } else {
                    // Found a file with this name, not a directory
                    return Ok(None);
                }
            } else {
                // Directory doesn't exist
                return Ok(None);
            }
        }

        Ok(Some(current_id))
    }

    /// Get or create a directory path using mkdir API.
    pub async fn ensure_path(&self, path: &str) -> Result<i64> {
        // First try to find existing path
        if let Some(id) = self.find_path_id(path).await? {
            return Ok(id);
        }

        // Path doesn't exist, create it using mkdir API
        let parts: Vec<&str> = path
            .trim_start_matches('/')
            .trim_end_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        let mut current_id: i64 = 0; // Root directory
        let mut current_path = String::new();

        for part in parts {
            current_path.push('/');
            current_path.push_str(part);

            // Check if this segment already exists
            if let Some(file) = self.find_file(current_id, part).await? {
                if file.is_folder() {
                    current_id = file.file_id;
                    // Update cache
                    {
                        let mut cache = self.dir_cache.write();
                        cache.insert(current_path.clone(), current_id);
                    }
                    continue;
                } else {
                    return Err(AppError::Internal(format!(
                        "Path component '{}' exists but is not a directory",
                        part
                    )));
                }
            }

            // Create the directory
            current_id = self.create_directory(current_id, part).await?;

            // Update cache
            {
                let mut cache = self.dir_cache.write();
                cache.insert(current_path.clone(), current_id);
            }
        }

        Ok(current_id)
    }

    /// Get the directory ID for a restic file type.
    pub async fn get_type_dir_id(&self, file_type: ResticFileType) -> Result<i64> {
        if file_type.is_config() {
            // Config is stored at repo root level
            self.ensure_path(&self.repo_path.clone()).await
        } else {
            let path = format!("{}/{}", self.repo_path, file_type.dirname());
            self.ensure_path(&path).await
        }
    }

    // ========================================================================
    // File Operations
    // ========================================================================

    /// Upload a file using single-step upload (for files <= 1GB).
    /// Uses duplicate=2 to overwrite existing files atomically.
    /// Updates the files cache if it exists for the parent directory.
    /// Includes 429 retry support.
    pub async fn upload_file(&self, parent_id: i64, filename: &str, data: Bytes) -> Result<i64> {
        let file_size = data.len() as i64;
        tracing::debug!(
            "Uploading file '{}' ({} bytes) to parent {}",
            filename,
            file_size,
            parent_id
        );

        // Calculate MD5 hash
        let md5_hash = format!("{:x}", md5::compute(&data));

        let upload_domain = self.get_upload_domain().await?;
        const MAX_RETRIES: usize = 3;
        const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(1);
        let upload_url = format!("{}/upload/v2/file/single/create", upload_domain);

        // Store data as Vec<u8> for reuse in retries
        let data_vec = data.to_vec();

        for attempt in 0..=MAX_RETRIES {
            let token = self.token_manager.get_token().await?;

            // Create multipart form with duplicate=2 for atomic overwrite
            let form = Form::new()
                .text("parentFileID", parent_id.to_string())
                .text("filename", filename.to_string())
                .text("etag", md5_hash.clone())
                .text("size", file_size.to_string())
                .text("duplicate", "2") // Overwrite existing file atomically
                .part(
                    "file",
                    Part::bytes(data_vec.clone()).file_name(filename.to_string()),
                );

            let response = self
                .token_manager
                .http_client()
                .post(&upload_url)
                .header("Authorization", format!("Bearer {}", token))
                .header("Platform", "open_platform")
                .multipart(form)
                .send()
                .await?;

            let status = response.status();
            let text = response.text().await?;

            tracing::debug!("Upload response status: {}, body: {}", status, text);

            let api_response: ApiResponse<SingleUploadData> = serde_json::from_str(&text)?;

            // Check for 429 rate limit error
            if api_response.code == 429 {
                if attempt < MAX_RETRIES {
                    tracing::warn!(
                        "Rate limited (429) when uploading file '{}', waiting {}s before retry (attempt {}/{})",
                        filename,
                        RETRY_DELAY.as_secs(),
                        attempt + 1,
                        MAX_RETRIES
                    );
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                } else {
                    tracing::error!(
                        "Rate limited (429) after {} retries when uploading file '{}', giving up",
                        MAX_RETRIES,
                        filename
                    );
                    return Err(AppError::Pan123Api {
                        code: api_response.code,
                        message: api_response.message,
                    });
                }
            }

            if !api_response.is_success() {
                return Err(AppError::Pan123Api {
                    code: api_response.code,
                    message: api_response.message,
                });
            }

            let upload_data = api_response
                .data
                .ok_or_else(|| AppError::Internal("No data in upload response".to_string()))?;

            if !upload_data.completed {
                return Err(AppError::Internal("Upload not completed".to_string()));
            }

            let file_id = upload_data.file_id;

            // Update files cache if it exists (Strategy A: only update if cache is initialized)
            {
                let mut cache = self.files_cache.write();
                if let Some(files) = cache.get_mut(&parent_id) {
                    // Check if file already exists (overwrite case)
                    if let Some(existing) = files.iter_mut().find(|f| f.filename == filename) {
                        // Update existing entry
                        existing.file_id = file_id;
                        existing.size = file_size;
                        tracing::debug!(
                            "Updated existing file '{}' in cache (id={}, size={})",
                            filename,
                            file_id,
                            file_size
                        );
                    } else {
                        // Add new entry
                        let new_file = FileInfo {
                            file_id,
                            filename: filename.to_string(),
                            file_type: 0, // 0 = file
                            size: file_size,
                            parent_file_id: parent_id,
                            trashed: 0,
                        };
                        files.push(new_file);
                        tracing::debug!(
                            "Added new file '{}' to cache (id={}, size={})",
                            filename,
                            file_id,
                            file_size
                        );
                    }
                }
            }

            tracing::info!("Uploaded file '{}' with id {}", filename, file_id);
            return Ok(file_id);
        }

        unreachable!()
    }

    /// Get download URL for a file.
    pub async fn get_download_url(&self, file_id: i64) -> Result<String> {
        let url = format!("{}/api/v1/file/download_info?fileId={}", BASE_URL, file_id);
        let response: ApiResponse<DownloadInfoData> = self.get(&url).await?;

        if !response.is_success() {
            if response.code == 5066 {
                return Err(AppError::NotFound(format!("File {} not found", file_id)));
            }
            return Err(AppError::Pan123Api {
                code: response.code,
                message: response.message,
            });
        }

        let data = response
            .data
            .ok_or_else(|| AppError::Internal("No data in download info response".to_string()))?;

        Ok(data.download_url)
    }

    /// Download a file's content with optional range support.
    /// Uses 123pan's native range download capability.
    pub async fn download_file(&self, file_id: i64, range: Option<(u64, u64)>) -> Result<Bytes> {
        let download_url = self.get_download_url(file_id).await?;

        let mut request = self.token_manager.http_client().get(&download_url);

        // Pass Range header to 123pan for native range support
        if let Some((start, end)) = range {
            request = request.header("Range", format!("bytes={}-{}", start, end));
        }

        let response = request.send().await?;

        if !response.status().is_success() && response.status().as_u16() != 206 {
            return Err(AppError::Internal(format!(
                "Download failed with status: {}",
                response.status()
            )));
        }

        Ok(response.bytes().await?)
    }

    /// Move a file to trash.
    pub async fn trash_file(&self, file_id: i64) -> Result<()> {
        tracing::debug!("Moving file {} to trash", file_id);

        let request = TrashRequest {
            file_ids: vec![file_id],
        };

        let response: ApiResponse<()> = self
            .post(&format!("{}/api/v1/file/trash", BASE_URL), &request)
            .await?;

        if !response.is_success() {
            return Err(AppError::Pan123Api {
                code: response.code,
                message: response.message,
            });
        }

        Ok(())
    }

    /// Permanently delete a file (must be in trash first).
    /// Updates the files cache if it exists for the parent directory.
    pub async fn delete_file(&self, parent_id: i64, file_id: i64) -> Result<()> {
        tracing::debug!(
            "Permanently deleting file {} from parent {}",
            file_id,
            parent_id
        );

        // First move to trash
        self.trash_file(file_id).await?;

        // Then permanently delete
        let request = DeleteRequest {
            file_ids: vec![file_id],
        };

        let response: ApiResponse<()> = self
            .post(&format!("{}/api/v1/file/delete", BASE_URL), &request)
            .await?;

        if !response.is_success() {
            return Err(AppError::Pan123Api {
                code: response.code,
                message: response.message,
            });
        }

        // Remove from files cache if it exists
        {
            let mut cache = self.files_cache.write();
            if let Some(files) = cache.get_mut(&parent_id) {
                let original_len = files.len();
                files.retain(|f| f.file_id != file_id);
                if files.len() < original_len {
                    tracing::debug!(
                        "Removed file {} from cache for parent {}",
                        file_id,
                        parent_id
                    );
                }
            }
        }

        tracing::info!("Deleted file {}", file_id);
        Ok(())
    }

    /// Check if a file exists and get its info using precise search.
    pub async fn get_file_info(&self, parent_id: i64, filename: &str) -> Result<Option<FileInfo>> {
        self.find_file(parent_id, filename).await
    }

    /// Initialize the repository structure.
    pub async fn init_repository(&self) -> Result<()> {
        tracing::info!("Initializing repository at {}", self.repo_path);

        // Create root directory
        self.ensure_path(&self.repo_path.clone()).await?;

        // Create type directories
        for file_type in [
            ResticFileType::Data,
            ResticFileType::Keys,
            ResticFileType::Locks,
            ResticFileType::Snapshots,
            ResticFileType::Index,
        ] {
            let path = format!("{}/{}", self.repo_path, file_type.dirname());
            self.ensure_path(&path).await?;
        }

        tracing::info!("Repository initialized successfully");
        Ok(())
    }

    /// Warm up the cache by pre-fetching all directory IDs and file listings.
    /// This should be called during startup before the server starts accepting requests.
    pub async fn warm_cache(&self) -> Result<()> {
        let start = std::time::Instant::now();
        tracing::info!("Starting cache warm-up for repository: {}", self.repo_path);

        // First, check if repo path exists and cache its ID
        let repo_id = match self.find_path_id(&self.repo_path).await? {
            Some(id) => {
                tracing::info!("Repository path found with id {}", id);
                id
            }
            None => {
                tracing::warn!(
                    "Repository path {} does not exist yet, skipping cache warm-up",
                    self.repo_path
                );
                return Ok(());
            }
        };

        // Pre-fetch file listing for repo root
        let root_files = self.list_files(repo_id).await?;
        tracing::info!("Cached {} items at repository root", root_files.len());

        // Pre-fetch each restic file type directory
        let file_types = [
            ResticFileType::Data,
            ResticFileType::Keys,
            ResticFileType::Locks,
            ResticFileType::Snapshots,
            ResticFileType::Index,
        ];

        for file_type in file_types {
            let path = format!("{}/{}", self.repo_path, file_type.dirname());
            if let Some(dir_id) = self.find_path_id(&path).await? {
                let files = self.list_files(dir_id).await?;
                tracing::info!("Cached {} files in /{}", files.len(), file_type.dirname());
            } else {
                tracing::debug!(
                    "Directory /{} does not exist, skipping",
                    file_type.dirname()
                );
            }
        }

        tracing::info!("Cache warm-up completed in {:?}", start.elapsed());
        Ok(())
    }
}

impl std::fmt::Debug for Pan123Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pan123Client")
            .field("token_manager", &self.token_manager)
            .field("repo_path", &self.repo_path)
            .finish()
    }
}
