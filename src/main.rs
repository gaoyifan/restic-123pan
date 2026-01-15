//! Restic REST API server backed by 123pan cloud storage.
//!
//! This server implements the Restic REST backend protocol and uses
//! 123pan as the underlying storage provider.

use clap::Parser;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use restic_123pan::config::Config;
use restic_123pan::pan123::Pan123Client;
use restic_123pan::restic::create_router;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse configuration
    let config = Config::parse();

    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.log_level.clone().into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting restic-123pan");
    tracing::info!("Repository path: {}", config.repo_path);
    tracing::info!("Listen address: {}", config.listen_addr);

    // Ensure database directory exists
    let db_path = std::path::Path::new(&config.db_path);
    if let Some(parent) = db_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let database_url = format!("sqlite:{}?mode=rwc", config.db_path);

    // Create 123pan client
    let client = Pan123Client::new(
        config.client_id.clone(),
        config.client_secret.clone(),
        config.repo_path.clone(),
        &database_url,
    )
    .await?;

    // Warm up the cache before starting the server
    tracing::info!("Checking file list cache...");
    client.warm_cache(config.force_cache_rebuild).await?;

    // Create router
    let app = create_router(client).layer(TraceLayer::new_for_http());

    // Parse listen address
    let addr: SocketAddr = config.listen_addr.parse()?;

    tracing::info!("Server listening on http://{}", addr);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
