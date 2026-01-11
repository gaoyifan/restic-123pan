//! Restic REST API v2 handlers.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, head, post},
    Router,
};
use serde::Deserialize;
use std::sync::Arc;

use super::types::FileEntryV2;
use crate::error::{AppError, Result};
use crate::pan123::{Pan123Client, ResticFileType};

/// Application state shared across handlers.
#[derive(Clone)]
pub struct AppState {
    pub client: Pan123Client,
}

/// Query parameters for repository creation.
#[derive(Debug, Deserialize)]
pub struct CreateQuery {
    #[serde(default)]
    pub create: Option<bool>,
}

/// Restic REST API v2 content type.
const V2_CONTENT_TYPE: &str = "application/vnd.x.restic.rest.v2";

/// Create the Axum router with all routes.
pub fn create_router(client: Pan123Client) -> Router {
    let state = Arc::new(AppState { client });

    Router::new()
        // Repository operations
        .route("/", post(create_repository).delete(delete_repository))
        // Config operations
        .route(
            "/config",
            head(head_config).get(get_config).post(post_config),
        )
        // Type directory listing
        .route("/:type/", get(list_files))
        // Individual file operations
        .route(
            "/:type/:name",
            head(head_file)
                .get(get_file)
                .post(post_file)
                .delete(delete_file),
        )
        .with_state(state)
}

// ============================================================================
// Repository Operations
// ============================================================================

/// POST /?create=true - Create repository.
async fn create_repository(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CreateQuery>,
) -> Result<impl IntoResponse> {
    if query.create != Some(true) {
        return Err(AppError::BadRequest(
            "Missing create=true parameter".to_string(),
        ));
    }

    tracing::info!("Creating repository");
    state.client.init_repository().await?;

    Ok(StatusCode::OK)
}

/// DELETE / - Delete repository (not implemented).
async fn delete_repository() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

// ============================================================================
// Config Operations
// ============================================================================

/// HEAD /config - Check if config exists.
async fn head_config(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse> {
    let dir_id = state.client.get_type_dir_id(ResticFileType::Config).await?;

    match state.client.get_file_info(dir_id, "config").await? {
        Some(file) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_LENGTH,
                file.size.to_string().parse().unwrap(),
            );
            Ok((StatusCode::OK, headers))
        }
        None => Err(AppError::NotFound("config".to_string())),
    }
}

/// GET /config - Get config file.
async fn get_config(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse> {
    let dir_id = state.client.get_type_dir_id(ResticFileType::Config).await?;

    let file = state
        .client
        .get_file_info(dir_id, "config")
        .await?
        .ok_or_else(|| AppError::NotFound("config".to_string()))?;

    let data = state.client.download_file(file.file_id, None).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "application/octet-stream".parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        data.len().to_string().parse().unwrap(),
    );

    Ok((headers, data))
}

/// POST /config - Save config file.
async fn post_config(
    State(state): State<Arc<AppState>>,
    body: axum::body::Body,
) -> Result<impl IntoResponse> {
    // Convert body to Bytes with 1GB limit
    let body = axum::body::to_bytes(body, 1024 * 1024 * 1024)
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read request body: {}", e)))?;

    tracing::info!("Saving config ({} bytes)", body.len());

    let dir_id = state.client.get_type_dir_id(ResticFileType::Config).await?;

    // With duplicate=2, upload will overwrite existing file atomically
    state.client.upload_file(dir_id, "config", body).await?;

    Ok(StatusCode::OK)
}

// ============================================================================
// File Listing
// ============================================================================

/// GET /{type}/ - List files of a type (v2 response only).
async fn list_files(
    State(state): State<Arc<AppState>>,
    Path(type_str): Path<String>,
) -> Result<Response> {
    let file_type = ResticFileType::from_str(&type_str)
        .ok_or_else(|| AppError::BadRequest(format!("Invalid type: {}", type_str)))?;

    if file_type.is_config() {
        return Err(AppError::BadRequest(
            "Use /config endpoint for config".to_string(),
        ));
    }

    let dir_id = state.client.get_type_dir_id(file_type).await?;
    let files = state.client.list_files(dir_id).await?;

    // Always return v2 format (name + size)
    let entries: Vec<FileEntryV2> = files
        .iter()
        .map(|f| FileEntryV2 {
            name: f.filename.clone(),
            size: f.size as u64,
        })
        .collect();

    let body = serde_json::to_string(&entries)?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, V2_CONTENT_TYPE)
        .body(Body::from(body))
        .unwrap())
}

// ============================================================================
// Individual File Operations
// ============================================================================

/// HEAD /{type}/{name} - Check if file exists.
async fn head_file(
    State(state): State<Arc<AppState>>,
    Path((type_str, name)): Path<(String, String)>,
) -> Result<impl IntoResponse> {
    let file_type = ResticFileType::from_str(&type_str)
        .ok_or_else(|| AppError::BadRequest(format!("Invalid type: {}", type_str)))?;

    let dir_id = state.client.get_type_dir_id(file_type).await?;

    match state.client.get_file_info(dir_id, &name).await? {
        Some(file) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_LENGTH,
                file.size.to_string().parse().unwrap(),
            );
            Ok((StatusCode::OK, headers))
        }
        None => Err(AppError::NotFound(name)),
    }
}

/// Parse Range header: bytes=start-end
fn parse_range(header: &str, file_size: u64) -> Option<(u64, u64)> {
    let range_spec = header.strip_prefix("bytes=")?;
    let parts: Vec<&str> = range_spec.split('-').collect();

    if parts.len() != 2 {
        return None;
    }

    let start: u64 = if parts[0].is_empty() {
        // bytes=-N means last N bytes
        let suffix_len: u64 = parts[1].parse().ok()?;
        file_size.saturating_sub(suffix_len)
    } else {
        parts[0].parse().ok()?
    };

    let end: u64 = if parts[1].is_empty() {
        file_size - 1
    } else {
        parts[1].parse().ok()?
    };

    if start <= end && start < file_size {
        Some((start, end.min(file_size - 1)))
    } else {
        None
    }
}

/// GET /{type}/{name} - Download file with native range support.
async fn get_file(
    State(state): State<Arc<AppState>>,
    Path((type_str, name)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<impl IntoResponse> {
    let file_type = ResticFileType::from_str(&type_str)
        .ok_or_else(|| AppError::BadRequest(format!("Invalid type: {}", type_str)))?;

    let dir_id = state.client.get_type_dir_id(file_type).await?;

    let file = state
        .client
        .get_file_info(dir_id, &name)
        .await?
        .ok_or_else(|| AppError::NotFound(name.clone()))?;

    let file_size = file.size as u64;

    // Check for Range header
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|r| parse_range(r, file_size));

    if let Some((start, end)) = range {
        // Use native range download from 123pan
        let data = state
            .client
            .download_file(file.file_id, Some((start, end)))
            .await?;

        let content_range = format!("bytes {}-{}/{}", start, end, file_size);

        let mut resp_headers = HeaderMap::new();
        resp_headers.insert(
            header::CONTENT_TYPE,
            "application/octet-stream".parse().unwrap(),
        );
        resp_headers.insert(
            header::CONTENT_LENGTH,
            data.len().to_string().parse().unwrap(),
        );
        resp_headers.insert(header::CONTENT_RANGE, content_range.parse().unwrap());

        Ok((StatusCode::PARTIAL_CONTENT, resp_headers, data).into_response())
    } else {
        // Full file download
        let data = state.client.download_file(file.file_id, None).await?;

        let mut resp_headers = HeaderMap::new();
        resp_headers.insert(
            header::CONTENT_TYPE,
            "application/octet-stream".parse().unwrap(),
        );
        resp_headers.insert(
            header::CONTENT_LENGTH,
            data.len().to_string().parse().unwrap(),
        );

        Ok((StatusCode::OK, resp_headers, data).into_response())
    }
}

/// POST /{type}/{name} - Upload file.
async fn post_file(
    State(state): State<Arc<AppState>>,
    Path((type_str, name)): Path<(String, String)>,
    body: axum::body::Body,
) -> Result<impl IntoResponse> {
    // Convert body to Bytes with 1GB limit
    let body = axum::body::to_bytes(body, 1024 * 1024 * 1024)
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read request body: {}", e)))?;

    let file_type = ResticFileType::from_str(&type_str)
        .ok_or_else(|| AppError::BadRequest(format!("Invalid type: {}", type_str)))?;

    tracing::info!("Uploading {}/{} ({} bytes)", type_str, name, body.len());

    let dir_id = state.client.get_type_dir_id(file_type).await?;

    // With duplicate=2, upload will overwrite existing file atomically
    state.client.upload_file(dir_id, &name, body).await?;

    Ok(StatusCode::OK)
}

/// DELETE /{type}/{name} - Delete file (idempotent).
async fn delete_file(
    State(state): State<Arc<AppState>>,
    Path((type_str, name)): Path<(String, String)>,
) -> Result<impl IntoResponse> {
    let file_type = ResticFileType::from_str(&type_str)
        .ok_or_else(|| AppError::BadRequest(format!("Invalid type: {}", type_str)))?;

    tracing::info!("Deleting {}/{}", type_str, name);

    let dir_id = state.client.get_type_dir_id(file_type).await?;

    // Idempotent: return OK even if file doesn't exist
    if let Some(file) = state.client.get_file_info(dir_id, &name).await? {
        state.client.delete_file(dir_id, file.file_id).await?;
    }

    Ok(StatusCode::OK)
}
