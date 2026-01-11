//! Error types for the restic-123pan application.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Application-wide error type.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 123pan API error
    #[error("123pan API error: {message}")]
    Pan123Api { code: i32, message: String },

    /// HTTP client error
    #[error("HTTP request failed: {0}")]
    HttpClient(#[from] reqwest::Error),

    /// Authentication error
    #[error("Authentication failed: {0}")]
    Auth(String),

    /// File not found
    #[error("File not found: {0}")]
    NotFound(String),

    /// Invalid request
    #[error("Invalid request: {0}")]
    BadRequest(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Pan123Api { code, message } => {
                tracing::error!("123pan API error: code={}, message={}", code, message);
                (StatusCode::BAD_GATEWAY, message.clone())
            }
            AppError::HttpClient(e) => {
                tracing::error!("HTTP client error: {}", e);
                (StatusCode::BAD_GATEWAY, e.to_string())
            }
            AppError::Auth(msg) => {
                tracing::error!("Auth error: {}", msg);
                (StatusCode::UNAUTHORIZED, msg.clone())
            }
            AppError::NotFound(msg) => {
                tracing::debug!("Not found: {}", msg);
                (StatusCode::NOT_FOUND, msg.clone())
            }
            AppError::BadRequest(msg) => {
                tracing::warn!("Bad request: {}", msg);
                (StatusCode::BAD_REQUEST, msg.clone())
            }
            AppError::Io(e) => {
                tracing::error!("IO error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            AppError::Json(e) => {
                tracing::error!("JSON error: {}", e);
                (StatusCode::BAD_REQUEST, e.to_string())
            }
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, msg.clone())
            }
        };

        let body = Json(json!({
            "error": message
        }));

        (status, body).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
