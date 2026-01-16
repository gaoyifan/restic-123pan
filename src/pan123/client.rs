//! 123pan API client for file operations.

use bytes::Bytes;
use parking_lot::RwLock;
use reqwest::multipart::{Form, Part};
use std::sync::Arc;

use super::auth::{TokenManager, BASE_URL};
use super::entity;
use super::types::*;
use crate::error::{AppError, Result};
use sea_orm::{
    entity::*,
    query::*,
    sea_query::{Expr, Index},
    *,
};

/// Macro to handle API retries for 429 (Rate Limit) and 401 (Unauthorized)
macro_rules! retry_api {
    ($self:expr, $request_maker:expr) => {{
        const MAX_RETRIES: usize = 3;
        const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(1);
        let mut final_response = None;

        for attempt in 0..=MAX_RETRIES {
            let token = $self.token_manager.get_token().await?;

            // Execute the request
            let response = $request_maker(&token).await?;

            // Parse response body as text first to handle potential debug logging and flexible parsing
            let text = response.text().await?;

            // Try to parse as JSON
            let api_response: ApiResponse<_> = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    return Err(AppError::Pan123Api {
                        code: -1,
                        message: format!("Failed to parse response JSON: {}", e)
                    });
                }
            };

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

            // Check for 401 unauthorized error (token expired)
            if api_response.code == 401 {
                if attempt < MAX_RETRIES {
                    tracing::warn!("Token expired (401), refreshing token and retrying (attempt {}/{})", attempt + 1, MAX_RETRIES);
                    if let Err(e) = $self.token_manager.refresh_token().await {
                        tracing::error!("Failed to refresh token on 401: {}", e);
                    }
                    continue;
                }
                // If max retries reached, fall through to return the 401 response
                // This prevents the panic by NOT continuing the loop, but returning the validation error
            }

            final_response = Some(api_response);
            break;
        }

        final_response.expect("Retry loop should always return a result")
    }};
}

/// Client for interacting with 123pan API.
#[derive(Clone)]
pub struct Pan123Client {
    token_manager: TokenManager,
    repo_path: String,
    /// Database connection for persistent cache
    pub(crate) db: DatabaseConnection,
    /// Upload domain (fetched dynamically)
    upload_domain: Arc<RwLock<Option<String>>>,
}

impl Pan123Client {
    /// Create a new 123pan client.
    pub async fn new(
        client_id: String,
        client_secret: String,
        repo_path: String,
        database_url: &str,
    ) -> Result<Self> {
        let mut opt = ConnectOptions::new(database_url.to_owned());
        opt.sqlx_logging_level(log::LevelFilter::Debug);

        let db = Database::connect(opt)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to connect to database: {}", e)))?;

        // Enable SQLite performance optimizations
        db.execute(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA cache_size=-256000; -- 256MB negative means pages
             PRAGMA temp_store=MEMORY;
             PRAGMA mmap_size=30000000000;",
        ))
        .await
        .map_err(|e| AppError::Internal(format!("Failed to set SQLite pragmas: {}", e)))?;

        let client = Self {
            token_manager: TokenManager::new(client_id, client_secret, db.clone()),
            repo_path,
            db,
            upload_domain: Arc::new(RwLock::new(None)),
        };

        client.init_db().await?;
        client.token_manager.init_db().await?;

        Ok(client)
    }

    /// Initialize database schema.
    async fn init_db(&self) -> Result<()> {
        let builder = self.db.get_database_backend();
        let schema = Schema::new(builder);

        let stmt = schema
            .create_table_from_entity(entity::Entity)
            .if_not_exists()
            .to_owned();
        self.db
            .execute(builder.build(&stmt))
            .await
            .map_err(|e| AppError::Internal(format!("Failed to initialize database: {}", e)))?;

        // Add composite unique index for lookup efficiency and name uniqueness
        let index_stmt = Index::create()
            .name("idx_parent_name")
            .table(entity::Entity)
            .col(entity::Column::ParentId)
            .col(entity::Column::Name)
            .unique()
            .if_not_exists()
            .to_owned();

        self.db
            .execute(builder.build(&index_stmt))
            .await
            .map_err(|e| AppError::Internal(format!("Failed to create index: {}", e)))?;

        Ok(())
    }

    /// Make an authenticated GET request with 429 retry support.
    /// Retries up to 3 times with 1 second delay on 429 rate limit errors.
    async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<ApiResponse<T>> {
        Ok(retry_api!(self, |token| {
            self.token_manager
                .http_client()
                .get(url)
                .header("Authorization", format!("Bearer {}", token))
                .header("Platform", "open_platform")
                .send()
        }))
    }

    /// Make an authenticated GET request without timeout.
    /// Used for file listing which can take a long time for large directories.
    /// Retries on 429 rate limit errors.
    async fn get_no_timeout<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<ApiResponse<T>> {
        Ok(retry_api!(self, |token| {
            self.token_manager
                .http_client()
                .get(url)
                .timeout(std::time::Duration::MAX) // No timeout
                .header("Authorization", format!("Bearer {}", token))
                .header("Platform", "open_platform")
                .send()
        }))
    }

    /// Make an authenticated POST request with JSON body and 429 retry support.
    /// Retries up to 3 times with 1 second delay on 429 rate limit errors.
    async fn post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<ApiResponse<T>> {
        // Serialize body once for reuse in retries
        let body_json = serde_json::to_string(body)?;

        Ok(retry_api!(self, |token| {
            self.token_manager
                .http_client()
                .post(url)
                .header("Authorization", format!("Bearer {}", token))
                .header("Platform", "open_platform")
                .header("Content-Type", "application/json")
                .body(body_json.clone())
                .send()
        }))
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
        let url = format!("{}/upload/v2/file/domain", BASE_URL);

        let api_response: ApiResponse<Vec<String>> = retry_api!(self, |token| {
            self.token_manager
                .http_client()
                .get(&url)
                .header("Authorization", format!("Bearer {}", token))
                .header("Platform", "open_platform")
                .send()
        });

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

        Ok(domain)
    }

    // ========================================================================
    // Directory Operations
    // ========================================================================

    /// List files in a directory.
    /// Returns files from the persistent cache.
    pub async fn list_files(&self, parent_id: i64) -> Result<Vec<FileInfo>> {
        let nodes = entity::Entity::find()
            .filter(entity::Column::ParentId.eq(parent_id))
            .all(&self.db)
            .await
            .map_err(|e| AppError::Internal(format!("DB error in list_files: {}", e)))?;

        Ok(nodes
            .into_iter()
            .map(|n| FileInfo {
                file_id: n.file_id,
                filename: n.name,
                file_type: if n.is_dir { 1 } else { 0 },
                size: n.size,
                parent_file_id: n.parent_id,
                trashed: 0,
            })
            .collect())
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
    /// Now a no-op as SQLite is always updated synchronously.
    pub fn invalidate_files_cache(&self, _parent_id: i64) {
        // No-op
    }

    /// Find a file by exact name in a directory.
    /// Uses directory listing instead of search (search has index delay issues).
    pub async fn find_file(&self, parent_id: i64, name: &str) -> Result<Option<FileInfo>> {
        let files = self.list_files(parent_id).await?;
        Ok(files.into_iter().find(|f| f.filename == name))
    }

    /// Create a directory. Returns the directory ID.
    /// If the directory already exists, returns its ID.
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
            if response.code == 1 {
                if let Some(existing) = self.find_file(parent_id, name).await? {
                    if existing.is_folder() {
                        let cached = entity::Entity::find()
                            .filter(entity::Column::ParentId.eq(parent_id))
                            .filter(entity::Column::Name.eq(name.to_string()))
                            .filter(entity::Column::IsDir.eq(true))
                            .one(&self.db)
                            .await
                            .map_err(|e| {
                                AppError::Internal(format!(
                                    "DB error checking existing directory: {}",
                                    e
                                ))
                            })?;

                        if cached.is_none() {
                            let existing_dir = entity::ActiveModel {
                                file_id: Set(existing.file_id),
                                parent_id: Set(parent_id),
                                name: Set(name.to_string()),
                                is_dir: Set(true),
                                size: Set(existing.size),
                                etag: Set(None),
                                updated_at: Set(chrono::Utc::now().naive_utc()),
                            };
                            existing_dir.insert(&self.db).await.map_err(|e| {
                                AppError::Internal(format!(
                                    "Failed to insert existing directory into DB: {}",
                                    e
                                ))
                            })?;
                        }

                        tracing::info!(
                            "Directory '{}' already exists with id {}",
                            name,
                            existing.file_id
                        );
                        return Ok(existing.file_id);
                    }
                }
                // Cache may be stale; refresh this directory from API and retry.
                let files = self.fetch_files_from_api(parent_id).await?;
                for f in &files {
                    entity::Entity::insert(entity::ActiveModel {
                        file_id: Set(f.file_id),
                        parent_id: Set(parent_id),
                        name: Set(f.filename.clone()),
                        is_dir: Set(f.is_folder()),
                        size: Set(f.size),
                        etag: Set(None),
                        updated_at: Set(chrono::Utc::now().naive_utc()),
                    })
                    .on_conflict(
                        sea_orm::sea_query::OnConflict::column(entity::Column::FileId)
                            .do_nothing()
                            .to_owned(),
                    )
                    .exec(&self.db)
                    .await
                    .map_err(|e| {
                        AppError::Internal(format!(
                            "Failed to refresh directory cache in mkdir fallback: {}",
                            e
                        ))
                    })?;
                }

                if let Some(existing) = files
                    .into_iter()
                    .find(|f| f.filename == name && f.is_folder())
                {
                    tracing::info!(
                        "Directory '{}' already exists with id {} (refreshed)",
                        name,
                        existing.file_id
                    );
                    return Ok(existing.file_id);
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

        // Add newly created directory to DB
        let new_dir = entity::ActiveModel {
            file_id: Set(data.dir_id),
            parent_id: Set(parent_id),
            name: Set(name.to_string()),
            is_dir: Set(true),
            size: Set(0),
            etag: Set(None),
            updated_at: Set(chrono::Utc::now().naive_utc()),
        };
        new_dir.insert(&self.db).await.map_err(|e| {
            AppError::Internal(format!("Failed to insert new directory into DB: {}", e))
        })?;

        tracing::info!("Created directory '{}' with id {}", name, data.dir_id);
        Ok(data.dir_id)
    }

    /// Find directory ID for a path by traversing from root.
    /// Uses SQLite queries.
    pub async fn find_path_id(&self, path: &str) -> Result<Option<i64>> {
        let parts: Vec<&str> = path
            .trim_start_matches('/')
            .trim_end_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        let mut current_id: i64 = 0; // Root directory

        for part in parts {
            let node = entity::Entity::find()
                .filter(entity::Column::ParentId.eq(current_id))
                .filter(entity::Column::Name.eq(part.to_string()))
                .filter(entity::Column::IsDir.eq(true))
                .one(&self.db)
                .await
                .map_err(|e| AppError::Internal(format!("DB error in find_path_id: {}", e)))?;

            if let Some(node) = node {
                current_id = node.file_id;
            } else {
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

        for part in parts {
            // Check if this segment already exists
            let node = entity::Entity::find()
                .filter(entity::Column::ParentId.eq(current_id))
                .filter(entity::Column::Name.eq(part.to_string()))
                .filter(entity::Column::IsDir.eq(true))
                .one(&self.db)
                .await
                .map_err(|e| AppError::Internal(format!("DB error in ensure_path: {}", e)))?;

            if let Some(node) = node {
                current_id = node.file_id;
                continue;
            }

            // Create the directory
            current_id = self.create_directory(current_id, part).await?;
        }

        Ok(current_id)
    }

    /// Extract the 2-character prefix from a filename for data subdirectory.
    /// Data files in restic are named by their hash, so the first 2 characters
    /// are used to create subdirectories for better listing performance.
    fn data_subdir_prefix(filename: &str) -> &str {
        // Data files are always hex hashes with at least 2 characters
        &filename[..2.min(filename.len())]
    }

    /// Get the directory ID for a data file, creating the 2-char subdirectory if needed.
    /// Data files are stored in `{repo_path}/data/{prefix}/` where prefix is the first 2 chars.
    pub async fn get_data_file_dir_id(&self, filename: &str) -> Result<i64> {
        let prefix = Self::data_subdir_prefix(filename);
        let path = format!("{}/data/{}", self.repo_path, prefix);
        self.ensure_path(&path).await
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
    /// Updates the persistent cache.
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
        let upload_url = format!("{}/upload/v2/file/single/create", upload_domain);

        // Store data as Vec<u8> for reuse in retries
        let data_vec = data.to_vec();

        let api_response: ApiResponse<SingleUploadData> = retry_api!(self, |token| {
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

            self.token_manager
                .http_client()
                .post(&upload_url)
                .header("Authorization", format!("Bearer {}", token))
                .header("Platform", "open_platform")
                .multipart(form)
                .send()
        });

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

        // Sync with DB (insert or replace by parent/name)
        entity::Entity::insert(entity::ActiveModel {
            file_id: Set(file_id),
            parent_id: Set(parent_id),
            name: Set(filename.to_string()),
            is_dir: Set(false),
            size: Set(file_size),
            etag: Set(Some(md5_hash.clone())),
            updated_at: Set(chrono::Utc::now().naive_utc()),
        })
        .on_conflict(
            sea_orm::sea_query::OnConflict::columns([
                entity::Column::ParentId,
                entity::Column::Name,
            ])
            .update_columns([
                entity::Column::FileId,
                entity::Column::ParentId,
                entity::Column::Name,
                entity::Column::Size,
                entity::Column::Etag,
                entity::Column::UpdatedAt,
            ])
            .to_owned(),
        )
        .exec(&self.db)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to sync file to DB: {}", e)))?;

        tracing::info!("Uploaded file '{}' with id {}", filename, file_id);
        return Ok(file_id);
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

        // Sync with DB: remove trashed file
        entity::Entity::delete_by_id(file_id)
            .exec(&self.db)
            .await
            .map_err(|e| {
                AppError::Internal(format!("Failed to delete trashed file from DB: {}", e))
            })?;

        Ok(())
    }

    /// Delete a file.
    pub async fn delete_file(&self, _parent_id: i64, file_id: i64) -> Result<()> {
        // First move to trash (required by 123pan for permanent deletion)
        self.trash_file(file_id).await?;

        let url = format!("{}/api/v1/file/delete", BASE_URL);
        let request = DeleteRequest {
            file_ids: vec![file_id],
        };

        let response: ApiResponse<serde_json::Value> = self.post(&url, &request).await?;

        if !response.is_success() {
            return Err(AppError::Pan123Api {
                code: response.code,
                message: response.message,
            });
        }

        // Remove from DB (already removed by trash_file, but safe to repeat)
        entity::Entity::delete_by_id(file_id)
            .exec(&self.db)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to delete file from DB: {}", e)))?;

        tracing::info!("Deleted file {} from persistent cache", file_id);
        Ok(())
    }

    /// Move files to a different directory.
    /// Supports up to 100 files per call (API limitation).
    pub async fn move_files(&self, file_ids: Vec<i64>, to_parent_id: i64) -> Result<()> {
        if file_ids.is_empty() {
            return Ok(());
        }

        tracing::debug!("Moving {} files to parent {}", file_ids.len(), to_parent_id);

        let request = MoveRequest {
            file_ids: file_ids.clone(),
            to_parent_file_id: to_parent_id,
        };

        let response: ApiResponse<()> = self
            .post(&format!("{}/api/v1/file/move", BASE_URL), &request)
            .await?;

        if !response.is_success() {
            return Err(AppError::Pan123Api {
                code: response.code,
                message: response.message,
            });
        }

        // Sync with DB: update parent_id for all moved files
        entity::Entity::update_many()
            .col_expr(entity::Column::ParentId, Expr::value(to_parent_id))
            .filter(entity::Column::FileId.is_in(file_ids.clone()))
            .exec(&self.db)
            .await
            .map_err(|e| {
                AppError::Internal(format!("Failed to update moved files in DB: {}", e))
            })?;

        tracing::info!("Moved {} files to parent {}", file_ids.len(), to_parent_id);
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
    /// If force_rebuild is false and the cache is not empty, it will reuse the existing cache.
    pub async fn warm_cache(&self, force_rebuild: bool) -> Result<()> {
        let start = std::time::Instant::now();

        // Check cache status
        let count = entity::Entity::find()
            .count(&self.db)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to count cache entries: {}", e)))?;

        if !force_rebuild && count > 0 {
            tracing::info!(
                "Reusing existing cache with {} entries for repository: {}",
                count,
                self.repo_path
            );
            return Ok(());
        }

        tracing::info!(
            "{} cache for repository: {}",
            if force_rebuild {
                "Rebuilding"
            } else {
                "Initializing"
            },
            self.repo_path
        );

        // Clear existing cache for a fresh start or if forced
        entity::Entity::delete_many()
            .exec(&self.db)
            .await
            .map_err(|e| AppError::Internal(format!("DB clear failed: {}", e)))?;

        // 1. Resolve repo_path root
        let parts: Vec<&str> = self
            .repo_path
            .trim_start_matches('/')
            .trim_end_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        let mut current_id: i64 = 0;
        for part in parts {
            let files = self.fetch_files_from_api(current_id).await?;
            let found = files
                .into_iter()
                .find(|f| f.filename == part && f.is_folder());

            match found {
                Some(found) => {
                    // Insert this path component into DB
                    entity::Entity::insert(entity::ActiveModel {
                        file_id: Set(found.file_id),
                        parent_id: Set(current_id),
                        name: Set(found.filename.clone()),
                        is_dir: Set(true),
                        size: Set(0),
                        etag: Set(None),
                        updated_at: Set(chrono::Utc::now().naive_utc()),
                    })
                    .exec(&self.db)
                    .await
                    .map_err(|e| {
                        AppError::Internal(format!(
                            "Failed to insert path component into DB: {}",
                            e
                        ))
                    })?;

                    current_id = found.file_id;
                }
                None => {
                    tracing::warn!(
                        "Path component {} not found during warm-up. Repository might not exist yet.",
                        part
                    );
                    return Ok(());
                }
            }
        }

        // 2. Recursively crawl everything under repo_path
        let mut queue = vec![(current_id, self.repo_path.clone())];

        while let Some((parent_id, path)) = queue.pop() {
            tracing::debug!("Crawling directory: {}", path);
            let files = self.fetch_files_from_api(parent_id).await?;

            if files.is_empty() {
                continue;
            }

            let mut models = Vec::with_capacity(files.len());
            for f in &files {
                models.push(entity::ActiveModel {
                    file_id: Set(f.file_id),
                    parent_id: Set(parent_id),
                    name: Set(f.filename.clone()),
                    is_dir: Set(f.is_folder()),
                    size: Set(f.size),
                    etag: Set(None),
                    updated_at: Set(chrono::Utc::now().naive_utc()),
                });
            }

            // SQLite parameter limit is usually 999.
            // Each row has 7 columns. 50 * 7 = 350, which is safe.
            for chunk in models.chunks(50) {
                entity::Entity::insert_many(chunk.to_vec())
                    .exec(&self.db)
                    .await
                    .map_err(|e| {
                        AppError::Internal(format!(
                            "Failed to batch insert files in warm_cache: {}",
                            e
                        ))
                    })?;
            }

            for f in files {
                if f.is_folder() {
                    queue.push((f.file_id, format!("{}/{}", path, f.filename)));
                }
            }
        }

        tracing::info!("Cache warm-up completed in {:?}", start.elapsed());
        Ok(())
    }
    /// List all data files across all 2-char subdirectories.
    /// Returns aggregated file list from all subdirectories under data/.
    pub async fn list_all_data_files(&self) -> Result<Vec<FileInfo>> {
        // Find data directory ID
        let Some(data_dir_id) = self
            .find_path_id(&format!("{}/data", self.repo_path))
            .await?
        else {
            return Ok(Vec::new());
        };

        // Find all subdirectories under /data
        let subdirs = entity::Entity::find()
            .filter(entity::Column::ParentId.eq(data_dir_id))
            .filter(entity::Column::IsDir.eq(true))
            .all(&self.db)
            .await
            .map_err(|e| {
                AppError::Internal(format!("DB error in list_all_data_files (subdirs): {}", e))
            })?;

        let subdir_ids: Vec<i64> = subdirs.into_iter().map(|n| n.file_id).collect();

        // Find all files in those subdirectories
        let files = entity::Entity::find()
            .filter(entity::Column::ParentId.is_in(subdir_ids))
            .filter(entity::Column::IsDir.eq(false))
            .all(&self.db)
            .await
            .map_err(|e| {
                AppError::Internal(format!("DB error in list_all_data_files (files): {}", e))
            })?;

        Ok(files
            .into_iter()
            .map(|n| FileInfo {
                file_id: n.file_id,
                filename: n.name,
                file_type: 0,
                size: n.size,
                parent_file_id: n.parent_id,
                trashed: 0,
            })
            .collect())
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
