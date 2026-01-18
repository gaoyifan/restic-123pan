//! Token management for 123pan API authentication.

use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use reqwest::Client;
use sea_orm::{
    sea_query::{ColumnDef, Expr, OnConflict, Query, Table},
    ConnectionTrait, DatabaseConnection,
};
use std::sync::Arc;

use super::types::{AccessTokenData, AccessTokenRequest, ApiResponse};
use super::{MAX_RETRIES, RETRY_DELAY};
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
    db: DatabaseConnection,
    token: Arc<RwLock<Option<TokenInfo>>>,
    last_refresh_time: Arc<RwLock<Option<DateTime<Utc>>>>,
}

const TOKEN_CACHE_TABLE: &str = "token_cache";
const TOKEN_CACHE_ID: &str = "id";
const TOKEN_CACHE_ACCESS_TOKEN: &str = "access_token";
const TOKEN_CACHE_EXPIRES_AT: &str = "expires_at";

impl TokenManager {
    /// Create a new token manager.
    pub fn new(client_id: String, client_secret: String, db: DatabaseConnection) -> Self {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client_id,
            client_secret,
            http_client,
            db,
            token: Arc::new(RwLock::new(None)),
            last_refresh_time: Arc::new(RwLock::new(None)),
        }
    }

    /// Initialize token cache table.
    pub async fn init_db(&self) -> Result<()> {
        let builder = self.db.get_database_backend();
        let stmt = Table::create()
            .table(TOKEN_CACHE_TABLE)
            .if_not_exists()
            .col(
                ColumnDef::new(TOKEN_CACHE_ID)
                    .integer()
                    .not_null()
                    .primary_key(),
            )
            .col(ColumnDef::new(TOKEN_CACHE_ACCESS_TOKEN).string().not_null())
            .col(ColumnDef::new(TOKEN_CACHE_EXPIRES_AT).string().not_null())
            .to_owned();

        self.db.execute(builder.build(&stmt)).await.map_err(|e| {
            AppError::Internal(format!("Failed to initialize token cache table: {}", e))
        })?;

        Ok(())
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

        // Check cached token in DB
        if let Some(token_info) = self.load_cached_token().await? {
            let mut token_guard = self.token.write();
            *token_guard = Some(token_info.clone());
            return Ok(token_info.access_token);
        }

        // Need to refresh token
        self.refresh_token().await
    }

    /// Force refresh the access token.
    /// Includes 429 retry support.
    /// Rate limited to once per minute.
    pub async fn refresh_token(&self) -> Result<String> {
        // Rate limit check
        {
            let last_refresh = self.last_refresh_time.read();
            if let Some(last_time) = *last_refresh {
                let now = Utc::now();
                if now - last_time < Duration::minutes(1) {
                    tracing::warn!(
                        "Token refresh rate limited (last refresh: {}), returning cached token if available",
                        last_time
                    );
                    // Try to return existing token even if potentially expired,
                    // or just return what we have to avoid spamming API.
                    // Ideally we should check if we really have a token.
                    let token_guard = self.token.read();
                    if let Some(ref token_info) = *token_guard {
                        return Ok(token_info.access_token.clone());
                    }
                    // If we don't have a token and we are rate limited, we might just have to error
                    // or wait. For now, returning error to signal we can't refresh yet is safer
                    // than spamming, but might cause downstream failures.
                    // Let's decide to return early.
                    return Err(AppError::Auth("Token refresh rate limited".to_string()));
                }
            }
        }

        tracing::info!("Refreshing 123pan access token");

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
                *token_guard = Some(token_info.clone());
            }

            self.store_cached_token(&token_info).await?;

            tracing::info!(
                "Successfully refreshed access token, expires at {}",
                expires_at
            );

            // Update last refresh time
            {
                let mut last_refresh = self.last_refresh_time.write();
                *last_refresh = Some(Utc::now());
            }

            return Ok(data.access_token);
        }

        unreachable!()
    }

    /// Get the HTTP client.
    pub fn http_client(&self) -> &Client {
        &self.http_client
    }

    async fn load_cached_token(&self) -> Result<Option<TokenInfo>> {
        let builder = self.db.get_database_backend();
        let stmt = Query::select()
            .columns([TOKEN_CACHE_ACCESS_TOKEN, TOKEN_CACHE_EXPIRES_AT])
            .from(TOKEN_CACHE_TABLE)
            .and_where(Expr::col(TOKEN_CACHE_ID).eq(1))
            .to_owned();

        let row = self
            .db
            .query_one(builder.build(&stmt))
            .await
            .map_err(|e| AppError::Internal(format!("Failed to query token cache: {}", e)))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let access_token: String = row
            .try_get("", TOKEN_CACHE_ACCESS_TOKEN)
            .map_err(|e| AppError::Internal(format!("Failed to read cached token: {}", e)))?;
        let expires_at_str: String = row
            .try_get("", TOKEN_CACHE_EXPIRES_AT)
            .map_err(|e| AppError::Internal(format!("Failed to read cached expiry: {}", e)))?;

        let expires_at = match DateTime::parse_from_rfc3339(&expires_at_str) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(_) => {
                tracing::warn!("Cached token expiry parse failed, ignoring cache");
                return Ok(None);
            }
        };

        let token_info = TokenInfo {
            access_token,
            expires_at,
        };

        if token_info.is_expired() {
            return Ok(None);
        }

        Ok(Some(token_info))
    }

    async fn store_cached_token(&self, token_info: &TokenInfo) -> Result<()> {
        let builder = self.db.get_database_backend();
        let stmt = Query::insert()
            .into_table(TOKEN_CACHE_TABLE)
            .columns([
                TOKEN_CACHE_ID,
                TOKEN_CACHE_ACCESS_TOKEN,
                TOKEN_CACHE_EXPIRES_AT,
            ])
            .values_panic([
                1.into(),
                token_info.access_token.clone().into(),
                token_info.expires_at.to_rfc3339().into(),
            ])
            .on_conflict(
                OnConflict::column(TOKEN_CACHE_ID)
                    .update_columns([TOKEN_CACHE_ACCESS_TOKEN, TOKEN_CACHE_EXPIRES_AT])
                    .to_owned(),
            )
            .to_owned();

        self.db
            .execute(builder.build(&stmt))
            .await
            .map_err(|e| AppError::Internal(format!("Failed to upsert token cache: {}", e)))?;

        Ok(())
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
