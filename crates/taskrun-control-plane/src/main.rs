//! TaskRun Control Plane Server

use std::net::SocketAddr;

use tonic::transport::{Identity, Server, ServerTlsConfig};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod config;
mod scheduler;
mod service;
mod state;

use config::Config;
use service::{RunServiceImpl, TaskServiceImpl, WorkerServiceImpl};
use state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Load config
    let config = Config::default();
    let addr: SocketAddr = config.bind_addr.parse()?;

    // Load TLS certificates
    let cert = std::fs::read(&config.tls_cert_path).map_err(|e| {
        format!(
            "Failed to read TLS certificate from '{}': {}. Run scripts/gen-dev-certs.sh first.",
            config.tls_cert_path, e
        )
    })?;
    let key = std::fs::read(&config.tls_key_path).map_err(|e| {
        format!(
            "Failed to read TLS key from '{}': {}. Run scripts/gen-dev-certs.sh first.",
            config.tls_key_path, e
        )
    })?;

    let identity = Identity::from_pem(cert, key);
    let tls_config = ServerTlsConfig::new().identity(identity);

    info!(bind_addr = %addr, "Starting TaskRun control plane with TLS");

    // Create shared state
    let state = AppState::new();

    // Create services
    let run_service = RunServiceImpl::new(state.clone()).into_server();
    let task_service = TaskServiceImpl::new(state.clone()).into_server();
    let worker_service = WorkerServiceImpl::new(state).into_server();

    // Start server with TLS
    Server::builder()
        .tls_config(tls_config)?
        .add_service(run_service)
        .add_service(task_service)
        .add_service(worker_service)
        .serve(addr)
        .await?;

    Ok(())
}
