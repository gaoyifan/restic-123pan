//! 123pan API client for file operations.

use bytes::Bytes;
use base64::Engine;
use reqwest::{Method, RequestBuilder, Url};

use super::auth::{TokenManager, BAPI_BASE_URL};
use super::entity;
use super::types::{
    ApiResponse, DownloadInfoData, FileInfo, FileListData, S3AuthData, UploadCompleteV2Data,
    UploadRequestData,
};
use super::{MAX_RETRIES, RETRY_DELAY};
use crate::error::{AppError, Result};
use crate::restic::ResticFileType;

use sea_orm::{
    entity::*,
    query::*,
    sea_query::{Expr, Index},
    *,
};

/// Client for interacting with 123pan API.
#[derive(Clone)]
pub struct Pan123Client {
    token_manager: TokenManager,
    repo_path: String,
    /// Database connection for persistent cache
    pub(crate) db: DatabaseConnection,
}

impl Pan123Client {
    fn sign_web_url(&self, raw_url: &str) -> Result<String> {
        const TABLE: &[u8; 26] = b"adefghlmyijnopkqrstubcvwsz";
        let mut url =
            Url::parse(raw_url).map_err(|e| AppError::Internal(format!("Invalid URL: {}", e)))?;

        let now = chrono::Utc::now()
            .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).expect("valid CST offset"));
        let timestamp = now.timestamp().to_string();
        let random = (now.timestamp_subsec_micros() % 10_000_000).to_string();
        let now_str = now.format("%Y%m%d%H%M").to_string();

        let mut mapped = Vec::with_capacity(now_str.len());
        for b in now_str.bytes() {
            if !b.is_ascii_digit() {
                return Err(AppError::Internal("Unexpected non-digit in time".to_string()));
            }
            mapped.push(TABLE[(b - b'0') as usize]);
        }

        let time_sign = crc32fast::hash(&mapped).to_string();
        let sign_data = format!(
            "{}|{}|{}|web|3|{}",
            timestamp,
            random,
            url.path(),
            time_sign
        );
        let data_sign = crc32fast::hash(sign_data.as_bytes()).to_string();
        url.query_pairs_mut().append_pair(
            &time_sign,
            &format!("{}-{}-{}", timestamp, random, data_sign),
        );

        Ok(url.to_string())
    }

    fn apply_web_headers(&self, request: RequestBuilder, token: &str) -> RequestBuilder {
        request
            .header("authorization", format!("Bearer {}", token))
            .header("origin", "https://www.123pan.com")
            .header("referer", "https://www.123pan.com/")
            .header("user-agent", "Mozilla/5.0 restic-123pan")
            .header("platform", "web")
            .header("app-version", "3")
    }

    async fn retry_api<T, F, Fut>(&self, request_maker: F) -> Result<ApiResponse<T>>
    where
        T: serde::de::DeserializeOwned,
        F: Fn(&str) -> Fut,
        Fut: std::future::Future<Output = std::result::Result<reqwest::Response, reqwest::Error>>,
    {
        for attempt in 0..=MAX_RETRIES {
            let token = self.token_manager.get_token().await?;
            let response = request_maker(&token).await?;
            let text = response.text().await?;

            let api_response: ApiResponse<T> = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    return Err(AppError::Pan123Api {
                        code: -1,
                        message: format!("Failed to parse response JSON: {}", e),
                    });
                }
            };

            if !api_response.is_success() {
                tracing::warn!("123pan API error response: {}", text);
            }

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
                }
                tracing::error!(
                    "Rate limited (429) after {} retries, giving up",
                    MAX_RETRIES
                );
                return Err(AppError::Pan123Api {
                    code: api_response.code,
                    message: api_response.message,
                });
            }

            if api_response.code == 401 {
                if attempt < MAX_RETRIES {
                    tracing::warn!(
                        "Token expired (401), refreshing token and retrying (attempt {}/{})",
                        attempt + 1,
                        MAX_RETRIES
                    );
                    if let Err(e) = self.token_manager.refresh_token().await {
                        tracing::error!("Failed to refresh token on 401: {}", e);
                    }
                    continue;
                }
            }

            return Ok(api_response);
        }

        Err(AppError::Internal(
            "Retry loop returned no response".to_string(),
        ))
    }

    /// Create a new 123pan client.
    pub async fn new(
        username: String,
        password: String,
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
            token_manager: TokenManager::new(username, password, db.clone()),
            repo_path,
            db,
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

        // Create indexes from entity definitions (#[sea_orm(indexed)] attributes)
        // create_index_from_entity generates CREATE INDEX statements, but doesn't support IF NOT EXISTS,
        // so we ignore "already exists" errors.
        for index_stmt in schema.create_index_from_entity(entity::Entity) {
            let sql = builder.build(&index_stmt);
            if let Err(e) = self.db.execute(sql).await {
                // Ignore "index already exists" errors (SQLite error code for this)
                let err_str = e.to_string();
                if !err_str.contains("already exists") {
                    return Err(AppError::Internal(format!(
                        "Failed to create index from entity: {}",
                        e
                    )));
                }
            }
        }

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

    async fn get_no_timeout<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<ApiResponse<T>> {
        self.get_with_timeout::<T>(url, Some(std::time::Duration::MAX))
            .await
    }

    async fn get_with_timeout<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        timeout: Option<std::time::Duration>,
    ) -> Result<ApiResponse<T>> {
        let signed_url = self.sign_web_url(url)?;
        self.retry_api(|token| {
            let mut request =
                self.apply_web_headers(self.token_manager.http_client().get(&signed_url), token);

            if let Some(timeout) = timeout {
                request = request.timeout(timeout);
            }

            request.send()
        })
        .await
    }

    async fn post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<ApiResponse<T>> {
        self.post_with_method(Method::POST, url, body).await
    }

    async fn post_with_method<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        method: Method,
        url: &str,
        body: &B,
    ) -> Result<ApiResponse<T>> {
        let body_json = serde_json::to_string(body)?;
        let signed_url = self.sign_web_url(url)?;

        self.retry_api(|token| {
            self.apply_web_headers(
                self.token_manager
                    .http_client()
                    .request(method.clone(), &signed_url),
                token,
            )
                .header("Content-Type", "application/json")
                .body(body_json.clone())
                .send()
        })
        .await
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

        Ok(nodes.into_iter().map(FileInfo::from).collect())
    }

    /// Fetch files from 123pan API (internal, bypasses cache).
    /// Uses no timeout to handle large directories with hundreds of thousands of files.
    async fn fetch_files_from_api(&self, parent_id: i64) -> Result<Vec<FileInfo>> {
        let mut all_files = Vec::new();
        let mut page = 1;
        let mut page_count = 0;

        loop {
            let url = format!(
                "{}/file/list/new?driveId=0&limit=100&next=0&orderBy=file_id&orderDirection=desc&parentFileId={}&trashed=false&SearchData=&Page={}&OnlyLookAbnormalFile=0&event=homeListFile&operateType=4&inDirectSpace=false",
                BAPI_BASE_URL, parent_id, page
            );

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
                page += 1;
            } else {
                break;
            }
        }

        Ok(all_files)
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

        let request = serde_json::json!({
            "driveId": 0,
            "etag": "",
            "fileName": name,
            "parentFileId": parent_id,
            "size": 0,
            "type": 1
        });

        let url = format!("{}/file/upload_request", BAPI_BASE_URL);

        let response: ApiResponse<UploadRequestData> = self.post(&url, &request).await?;

        if !response.is_success() {
            tracing::debug!(
                "mkdir failed: code={}, message='{}'",
                response.code,
                response.message
            );
            // Old API returns non-zero for duplicate names.
            if response.code != 0 {
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
                        etag: Set(f.etag.clone()),
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

        let upload_data = response
            .data
            .ok_or_else(|| AppError::Internal("No data in mkdir response".to_string()))?;
        let info = upload_data
            .info
            .ok_or_else(|| AppError::Internal("No directory info in mkdir response".to_string()))?;
        let dir_id = info.file_id;

        // Add newly created directory to DB
        let new_dir = entity::ActiveModel {
            file_id: Set(dir_id),
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

        tracing::info!("Created directory '{}' with id {}", name, dir_id);
        Ok(dir_id)
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

    /// Upload a file through the web-client API flow:
    /// upload_request -> s3_upload_object/auth -> PUT -> upload_complete/v2.
    pub async fn upload_file(&self, parent_id: i64, filename: &str, data: Bytes) -> Result<i64> {
        let file_size = data.len() as i64;
        tracing::debug!(
            "Uploading file '{}' ({} bytes) to parent {}",
            filename,
            file_size,
            parent_id
        );

        let md5_hash = format!("{:x}", md5::compute(&data));
        let upload_req_url = format!("{}/file/upload_request", BAPI_BASE_URL);
        let upload_req_body = serde_json::json!({
            "driveId": 0,
            "duplicate": 2,
            "etag": md5_hash.clone(),
            "fileName": filename,
            "parentFileId": parent_id,
            "size": file_size,
            "type": 0
        });
        let api_response: ApiResponse<UploadRequestData> =
            self.post(&upload_req_url, &upload_req_body).await?;

        if !api_response.is_success() {
            return Err(AppError::Pan123Api {
                code: api_response.code,
                message: api_response.message,
            });
        }

        let upload_data = api_response
            .data
            .ok_or_else(|| AppError::Internal("No data in upload request response".to_string()))?;

        let file_id = if upload_data.reuse {
            upload_data
                .info
                .as_ref()
                .map(|f| f.file_id)
                .or_else(|| {
                    if upload_data.file_id > 0 {
                        Some(upload_data.file_id)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| AppError::Internal("No file id in reuse upload response".to_string()))?
        } else {
            let s3_auth_url = format!("{}/file/s3_upload_object/auth", BAPI_BASE_URL);
            let s3_auth_body = serde_json::json!({
                "StorageNode": upload_data.storage_node,
                "bucket": upload_data.bucket,
                "key": upload_data.key,
                "partNumberEnd": 1,
                "partNumberStart": 1,
                "uploadId": upload_data.upload_id
            });
            let s3_auth_response: ApiResponse<S3AuthData> =
                self.post(&s3_auth_url, &s3_auth_body).await?;
            if !s3_auth_response.is_success() {
                return Err(AppError::Pan123Api {
                    code: s3_auth_response.code,
                    message: s3_auth_response.message,
                });
            }
            let presigned_url = s3_auth_response
                .data
                .and_then(|d| d.presigned_urls.get("1").cloned())
                .ok_or_else(|| AppError::Internal("No presigned URL for upload".to_string()))?;

            let upload_res = self
                .token_manager
                .http_client()
                .put(&presigned_url)
                .header("content-length", file_size.to_string())
                .body(data.clone())
                .send()
                .await?;

            if !upload_res.status().is_success() {
                return Err(AppError::Internal(format!(
                    "S3 upload failed with status {}",
                    upload_res.status()
                )));
            }

            let complete_url = format!("{}/file/upload_complete/v2", BAPI_BASE_URL);
            let complete_body = serde_json::json!({
                "StorageNode": upload_data.storage_node,
                "bucket": upload_data.bucket,
                "fileId": upload_data.file_id,
                "fileSize": file_size,
                "isMultipart": false,
                "key": upload_data.key,
                "uploadId": upload_data.upload_id
            });
            let complete_response: ApiResponse<UploadCompleteV2Data> =
                self.post(&complete_url, &complete_body).await?;
            if !complete_response.is_success() {
                return Err(AppError::Pan123Api {
                    code: complete_response.code,
                    message: complete_response.message,
                });
            }
            complete_response
                .data
                .ok_or_else(|| AppError::Internal("No data in upload complete response".to_string()))?
                .file_info
                .file_id
        };

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
        let node = entity::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .map_err(|e| AppError::Internal(format!("DB error in get_download_url: {}", e)))?
            .ok_or_else(|| AppError::NotFound(format!("File {} not found in cache", file_id)))?;

        let mut etag = node.etag.unwrap_or_default();
        let mut s3_key_flag = String::new();

        let remote = self.fetch_files_from_api(node.parent_id).await?;
        if let Some(remote_file) = remote.into_iter().find(|f| f.file_id == file_id) {
            if etag.is_empty() {
                etag = remote_file.etag.unwrap_or_default();
            }
            s3_key_flag = remote_file.s3_key_flag.unwrap_or_default();
        }

        let body = serde_json::json!({
            "driveId": 0,
            "etag": etag,
            "fileId": file_id,
            "fileName": node.name,
            "s3keyFlag": s3_key_flag,
            "size": node.size,
            "type": 0
        });
        let url = format!("{}/file/download_info", BAPI_BASE_URL);
        let response: ApiResponse<DownloadInfoData> = self.post(&url, &body).await?;

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
        let mut resolved_url = download_url.clone();

        if let Ok(url) = Url::parse(&download_url) {
            if let Some(params) = url
                .query_pairs()
                .find(|(k, _)| k == "params")
                .map(|(_, v)| v.into_owned())
            {
                if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(params) {
                    if let Ok(real_url) = String::from_utf8(decoded) {
                        resolved_url = real_url;
                    }
                }
            }
        }

        let mut probe_request = self
            .token_manager
            .http_client()
            .get(&resolved_url)
            .header("referer", "https://www.123pan.com/");
        if let Some((start, end)) = range {
            probe_request = probe_request.header("Range", format!("bytes={}-{}", start, end));
        }

        let probe = probe_request.send().await?;
        let final_url = if probe.status().is_redirection() {
            probe
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
                .unwrap_or(resolved_url)
        } else if probe.status().is_success() {
            let text = probe.text().await?;
            serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|j| {
                    j.get("data")
                        .and_then(|d| d.get("redirect_url"))
                        .and_then(|u| u.as_str())
                        .map(|u| u.to_string())
                })
                .unwrap_or(resolved_url)
        } else {
            return Err(AppError::Internal(format!(
                "Download probe failed with status: {}",
                probe.status()
            )));
        };

        let mut request = self
            .token_manager
            .http_client()
            .get(&final_url)
            .header("referer", "https://www.123pan.com/");
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

        let request = serde_json::json!({
            "driveId": 0,
            "operation": true,
            "fileTrashInfoList": [{"FileId": file_id}]
        });

        let response: ApiResponse<serde_json::Value> = self
            .post(&format!("{}/file/trash", BAPI_BASE_URL), &request)
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
        // Web API handles deletion through trash API.
        self.trash_file(file_id).await?;

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

        let file_id_list: Vec<_> = file_ids
            .iter()
            .map(|id| serde_json::json!({ "FileId": id }))
            .collect();
        let request = serde_json::json!({
            "fileIdList": file_id_list,
            "parentFileId": to_parent_id
        });

        let response: ApiResponse<serde_json::Value> = self
            .post(&format!("{}/file/mod_pid", BAPI_BASE_URL), &request)
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
    /// Resumes from where it left off if interrupted.
    pub async fn warm_cache(&self, force_rebuild: bool) -> Result<()> {
        let start = std::time::Instant::now();

        tracing::info!(
            "{} cache for repository: {}",
            if force_rebuild {
                "Rebuilding"
            } else {
                "Warming up (resumable)"
            },
            self.repo_path
        );

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
            // We use fetch_or_use_cache here to avoid API calls if the path is already cached
            let (files, cached) = self.fetch_or_use_cache(current_id, force_rebuild).await?;
            
            let found = files
                .into_iter()
                .find(|f| f.filename == part && f.is_folder());

            match found {
                Some(found) => {
                    if !cached {
                         tracing::debug!("Found path component '{}' via API", part);
                    }
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
        let mut fetched_count = 0;
        let mut cached_count = 0;

        while let Some((parent_id, path)) = queue.pop() {
            let (files, cached) = self.fetch_or_use_cache(parent_id, force_rebuild).await?;
            
            if cached {
                cached_count += 1;
                tracing::debug!("Cache hit for directory: {}", path);
            } else {
                fetched_count += 1;
                tracing::info!("Fetched directory: {} ({} files)", path, files.len());
            }

            for f in files {
                if f.is_folder() {
                    queue.push((f.file_id, format!("{}/{}", path, f.filename)));
                }
            }
        }

        tracing::info!(
            "Cache warm-up completed in {:?}. Fetched {} dirs, cached {} dirs.", 
            start.elapsed(),
            fetched_count,
            cached_count
        );
        Ok(())
    }

    async fn cache_has_children(&self, parent_id: i64) -> Result<bool> {
        let count = entity::Entity::find()
            .filter(entity::Column::ParentId.eq(parent_id))
            .count(&self.db)
            .await
            .map_err(|e| AppError::Internal(format!("DB count fail: {}", e)))?;
        Ok(count > 0)
    }

    async fn fetch_or_use_cache(
        &self,
        parent_id: i64,
        force_rebuild: bool,
    ) -> Result<(Vec<FileInfo>, bool)> {
        if !force_rebuild && self.cache_has_children(parent_id).await? {
            let files = self.list_files(parent_id).await?;
            return Ok((files, true));
        }

        let files = self.fetch_files_from_api(parent_id).await?;
        self.save_files_to_db(parent_id, &files).await?;
        Ok((files, false))
    }

    async fn save_files_to_db(&self, parent_id: i64, files: &[FileInfo]) -> Result<()> {
        let txn = self
            .db
            .begin()
            .await
            .map_err(|e| AppError::Internal(format!("DB begin fail: {}", e)))?;

        // Delete existing entries for this parent to avoid stale entries
        entity::Entity::delete_many()
            .filter(entity::Column::ParentId.eq(parent_id))
            .exec(&txn)
            .await
            .map_err(|e| AppError::Internal(format!("DB delete fail: {}", e)))?;

        if !files.is_empty() {
             let mut models = Vec::with_capacity(files.len());
             for f in files {
                models.push(entity::ActiveModel {
                    file_id: Set(f.file_id),
                    parent_id: Set(parent_id),
                    name: Set(f.filename.clone()),
                    is_dir: Set(f.is_folder()),
                    size: Set(f.size),
                    etag: Set(f.etag.clone()),
                    updated_at: Set(chrono::Utc::now().naive_utc()),
                });
            }

            // Chunking for SQLite limits
            for chunk in models.chunks(50) {
                entity::Entity::insert_many(chunk.to_vec())
                    .exec(&txn)
                    .await
                    .map_err(|e| {
                        AppError::Internal(format!("DB batch insert fail: {}", e))
                    })?;
            }
        }

        txn.commit()
            .await
            .map_err(|e| AppError::Internal(format!("DB commit fail: {}", e)))?;
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

        Ok(files.into_iter().map(FileInfo::from).collect())
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
