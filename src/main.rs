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

    // Create 123pan client
    let client = Pan123Client::new(
        config.client_id.clone(),
        config.client_secret.clone(),
        config.repo_path.clone(),
    );

    // Warm up the cache before starting the server
    tracing::info!("Warming up file list cache...");
    client.warm_cache().await?;

    // Create router
    let app = create_router(client)
        .layer(TraceLayer::new_for_http());

    // Parse listen address
    let addr: SocketAddr = config.listen_addr.parse()?;

    tracing::info!("Server listening on http://{}", addr);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
