//! TaskRun Worker Daemon

use std::sync::Arc;
use std::time::Duration;

use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

mod config;
mod connection;
mod executor;

use config::Config;
use connection::WorkerConnection;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Load config
    let config = Arc::new(Config::default());

    info!(
        worker_id = %config.worker_id,
        control_plane = %config.control_plane_addr,
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
