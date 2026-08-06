//! Token management for 123pan web API authentication.

use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use reqwest::Client;
use sea_orm::{
    sea_query::{ColumnDef, Expr, OnConflict, Query, Table},
    ConnectionTrait, DatabaseConnection,
};
use std::sync::Arc;

use super::types::{SignInRequest, SignInResponse};
use super::{MAX_RETRIES, RETRY_DELAY};
use crate::error::{AppError, Result};

/// Base URL for 123pan web APIs.
pub const BASE_URL: &str = "https://yun.123pan.com";
pub const BAPI_BASE_URL: &str = "https://yun.123pan.com/b/api";
pub const LOGIN_URL: &str = "https://login.123pan.com/api/user/sign_in";

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

/// Token manager that handles automatic sign-in and token caching.
#[derive(Clone)]
pub struct TokenManager {
    username: String,
    password: String,
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
    pub fn new(username: String, password: String, db: DatabaseConnection) -> Self {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            username,
            password,
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
        {
            let token_guard = self.token.read();
            if let Some(ref token_info) = *token_guard {
                if !token_info.is_expired() {
                    return Ok(token_info.access_token.clone());
                }
            }
        }

        if let Some(token_info) = self.load_cached_token().await? {
            let mut token_guard = self.token.write();
            *token_guard = Some(token_info.clone());
            return Ok(token_info.access_token);
        }

        self.refresh_token().await
    }

    /// Force refresh token by sign-in.
    /// Rate limited to once per minute to avoid frequent sign-in.
    pub async fn refresh_token(&self) -> Result<String> {
        {
            let last_refresh = self.last_refresh_time.read();
            if let Some(last_time) = *last_refresh {
                let now = Utc::now();
                if now - last_time < Duration::minutes(1) {
                    let token_guard = self.token.read();
                    if let Some(ref token_info) = *token_guard {
                        return Ok(token_info.access_token.clone());
                    }
                    return Err(AppError::Auth("Token refresh rate limited".to_string()));
                }
            }
        }

        tracing::info!("Refreshing 123pan access token via sign-in");

        let request_json = serde_json::to_string(&SignInRequest {
            passport: self.username.clone(),
            password: self.password.clone(),
            remember: true,
        })
        .map_err(|e| AppError::Auth(format!("Failed to serialize sign-in request: {}", e)))?;

        for attempt in 0..=MAX_RETRIES {
            let response = self
                .http_client
                .post(LOGIN_URL)
                .header("origin", BASE_URL)
                .header("referer", format!("{}/", BASE_URL))
                .header("user-agent", "Mozilla/5.0 restic-123pan")
                .header("platform", "web")
                .header("app-version", "3")
                .header("content-type", "application/json")
                .body(request_json.clone())
                .send()
                .await?;

            let sign_in_response: SignInResponse = response.json().await?;

            if sign_in_response.code == 429 {
                if attempt < MAX_RETRIES {
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                }
                return Err(AppError::Auth(format!(
                    "Failed to sign in after retries: {} (code: {})",
                    sign_in_response.message, sign_in_response.code
                )));
            }

            if sign_in_response.code != 200 {
                return Err(AppError::Auth(format!(
                    "Failed to sign in: {} (code: {})",
                    sign_in_response.message, sign_in_response.code
                )));
            }

            let data = sign_in_response
                .data
                .ok_or_else(|| AppError::Auth("No data in sign-in response".to_string()))?;

            let expires_at = DateTime::parse_from_rfc3339(&data.expire)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now() + Duration::days(90));

            let token_info = TokenInfo {
                access_token: data.token.clone(),
                expires_at,
            };

            {
                let mut token_guard = self.token.write();
                *token_guard = Some(token_info.clone());
            }

            self.store_cached_token(&token_info).await?;

            {
                let mut last_refresh = self.last_refresh_time.write();
                *last_refresh = Some(Utc::now());
            }

            return Ok(data.token);
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
            Err(_) => return Ok(None),
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
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{BAPI_BASE_URL, BASE_URL};

    #[test]
    fn uses_current_123pan_web_api_host() {
        assert_eq!(BASE_URL, "https://yun.123pan.com");
        assert_eq!(BAPI_BASE_URL, "https://yun.123pan.com/b/api");
    }
}
