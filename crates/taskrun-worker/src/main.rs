//! TaskRun Worker Daemon

use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod config;
mod connection;
mod executor;

use config::{Cli, Config};
use connection::WorkerConnection;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Initialize tracing with log level from CLI
    let filter = EnvFilter::try_new(&cli.log_level).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();

    // Build config from CLI
    let config = Arc::new(Config::from_cli(&cli));

    info!(
        worker_id = %config.worker_id,
        control_plane = %config.control_plane_addr,
        agent = %config.agent_name,
        model = format!("{}/{}", config.model_provider, config.model_name),
        allowed_tools = ?config.allowed_tools,
        denied_tools = ?config.denied_tools,
        "Starting TaskRun worker"
    );

    // Reconnection loop
    loop {
        let mut connection = WorkerConnection::new(config.clone());

        match connection.connect_and_run().await {
            Ok(_) => {
                info!("Connection closed normally");
            }
            Err(e) => {
                error!(error = %e, "Connection error");
            }
        }

        info!(
            delay_secs = config.reconnect_delay_secs,
            "Reconnecting in {} seconds...", config.reconnect_delay_secs
        );
        tokio::time::sleep(Duration::from_secs(config.reconnect_delay_secs)).await;
    }
}
