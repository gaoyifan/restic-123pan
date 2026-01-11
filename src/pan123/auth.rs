//! Token management for 123pan API authentication.

use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use reqwest::Client;
use std::sync::Arc;

use super::types::{AccessTokenData, AccessTokenRequest, ApiResponse};
use crate::error::{AppError, Result};

/// Base URL for 123pan Open Platform API.
pub const BASE_URL: &str = "https://open-api.123pan.com";

/// Token with expiry information.
#[derive(Debug, Clone)]
struct TokenInfo {
    access_token: String,
    expires_at: DateTime<Utc>,
}

impl TokenInfo {
    /// Check if the token is expired or about to expire (with 5 minute buffer).
    fn is_expired(&self) -> bool {
        Utc::now() + Duration::minutes(5) >= self.expires_at
    }
}

/// Token manager that handles automatic token refresh.
#[derive(Clone)]
pub struct TokenManager {
    client_id: String,
    client_secret: String,
    http_client: Client,
    token: Arc<RwLock<Option<TokenInfo>>>,
}

impl TokenManager {
    /// Create a new token manager.
    pub fn new(client_id: String, client_secret: String) -> Self {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client_id,
            client_secret,
            http_client,
            token: Arc::new(RwLock::new(None)),
        }
    }

    /// Get a valid access token, refreshing if necessary.
    pub async fn get_token(&self) -> Result<String> {
        // Check if we have a valid token
        {
            let token_guard = self.token.read();
            if let Some(ref token_info) = *token_guard {
                if !token_info.is_expired() {
                    return Ok(token_info.access_token.clone());
                }
            }
        }

        // Need to refresh token
        self.refresh_token().await
    }

    /// Force refresh the access token.
    /// Includes 429 retry support.
    async fn refresh_token(&self) -> Result<String> {
        tracing::info!("Refreshing 123pan access token");

        const MAX_RETRIES: usize = 3;
        const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(1);
        let url = format!("{}/api/v1/access_token", BASE_URL);

        let request = AccessTokenRequest {
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
        };

        // Serialize request once for reuse in retries
        let request_json = serde_json::to_string(&request).map_err(|e| {
            AppError::Auth(format!("Failed to serialize access token request: {}", e))
        })?;

        for attempt in 0..=MAX_RETRIES {
            let response = self
                .http_client
                .post(&url)
                .header("Platform", "open_platform")
                .header("Content-Type", "application/json")
                .body(request_json.clone())
                .send()
                .await?;

            let api_response: ApiResponse<AccessTokenData> = response.json().await?;

            // Check for 429 rate limit error
            if api_response.code == 429 {
                if attempt < MAX_RETRIES {
                    tracing::warn!(
                        "Rate limited (429) when refreshing access token, waiting {}s before retry (attempt {}/{})",
                        RETRY_DELAY.as_secs(),
                        attempt + 1,
                        MAX_RETRIES
                    );
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                } else {
                    tracing::error!(
                        "Rate limited (429) after {} retries when refreshing access token, giving up",
                        MAX_RETRIES
                    );
                    return Err(AppError::Auth(format!(
                        "Failed to get access token after retries: {} (code: {})",
                        api_response.message, api_response.code
                    )));
                }
            }

            if !api_response.is_success() {
                return Err(AppError::Auth(format!(
                    "Failed to get access token: {} (code: {})",
                    api_response.message, api_response.code
                )));
            }

            let data = api_response
                .data
                .ok_or_else(|| AppError::Auth("No data in access token response".to_string()))?;

            // Parse expiry time
            let expires_at = DateTime::parse_from_rfc3339(&data.expired_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| {
                    // Default to 1 hour from now if parsing fails
                    tracing::warn!("Failed to parse token expiry time, defaulting to 1 hour");
                    Utc::now() + Duration::hours(1)
                });

            let token_info = TokenInfo {
                access_token: data.access_token.clone(),
                expires_at,
            };

            // Update stored token
            {
                let mut token_guard = self.token.write();
                *token_guard = Some(token_info);
            }

            tracing::info!(
                "Successfully refreshed access token, expires at {}",
                expires_at
            );

            return Ok(data.access_token);
        }

        unreachable!()
    }

    /// Get the HTTP client.
    pub fn http_client(&self) -> &Client {
        &self.http_client
    }
}

impl std::fmt::Debug for TokenManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenManager")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .finish()
    }
}
