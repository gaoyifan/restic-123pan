//! Configuration handling for the application.

use clap::Parser;

/// Restic REST API server backed by 123pan cloud storage.
#[derive(Parser, Debug, Clone)]
#[command(name = "restic-123pan")]
#[command(about = "Restic REST API backend server using 123pan cloud storage")]
pub struct Config {
    /// 123pan client ID
    #[arg(long, env = "PAN123_CLIENT_ID")]
    pub client_id: String,

    /// 123pan client secret
    #[arg(long, env = "PAN123_CLIENT_SECRET")]
    pub client_secret: String,

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
