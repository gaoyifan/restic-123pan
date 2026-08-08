//! Configuration handling for the application.

use clap::Parser;
use std::path::PathBuf;

/// Restic REST API server backed by 123pan cloud storage.
#[derive(Parser, Debug, Clone)]
#[command(name = "restic-123pan")]
#[command(about = "Restic REST API backend server using 123pan cloud storage")]
pub struct Config {
    /// 123pan account username (phone/email)
    #[arg(long, env = "PAN123_USERNAME")]
    pub username: Option<String>,

    /// File containing the 123pan username.
    #[arg(long, conflicts_with = "username", env = "PAN123_USERNAME_FILE")]
    pub username_file: Option<PathBuf>,

    /// 123pan account password
    #[arg(long, env = "PAN123_PASSWORD")]
    pub password: Option<String>,

    /// File containing the 123pan password.
    #[arg(long, conflicts_with = "password", env = "PAN123_PASSWORD_FILE")]
    pub password_file: Option<PathBuf>,

    /// Root folder path on 123pan for the repository
    #[arg(long, env = "PAN123_REPO_PATH", default_value = "/restic-backup")]
    pub repo_path: String,

    /// Server listen address (host or IP)
    #[arg(long, env = "LISTEN_ADDR", default_value = "127.0.0.1")]
    pub listen_addr: String,

    /// Server listen port
    #[arg(long, env = "LISTEN_PORT", default_value_t = 8000)]
    pub listen_port: u16,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, env = "RUST_LOG", default_value = "info")]
    pub log_level: String,

    /// Path to the SQLite database file
    #[arg(long, env = "DB_PATH", default_value = "cache-123pan.db")]
    pub db_path: String,

    /// Force rebuild of the file list cache on startup
    #[arg(long, env = "FORCE_CACHE_REBUILD", default_value = "false")]
    pub force_cache_rebuild: bool,
}

impl Config {
    pub fn load_credentials(mut self) -> anyhow::Result<Self> {
        if self.username.is_none() {
            self.username = self
                .username_file
                .as_ref()
                .map(std::fs::read_to_string)
                .transpose()?
                .map(|value| value.trim().to_owned());
        }
        if self.password.is_none() {
            self.password = self
                .password_file
                .as_ref()
                .map(std::fs::read_to_string)
                .transpose()?
                .map(|value| value.trim().to_owned());
        }
        Ok(self)
    }

    pub fn credentials(self) -> anyhow::Result<(String, String)> {
        let username = self
            .username
            .ok_or_else(|| anyhow::anyhow!("a 123pan username or username file is required"))?;
        let password = self
            .password
            .ok_or_else(|| anyhow::anyhow!("a 123pan password or password file is required"))?;
        Ok((username, password))
    }
}
