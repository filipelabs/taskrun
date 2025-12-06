//! TaskRun Control Plane Server

use std::net::SocketAddr;

use tonic::transport::Server;
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

    info!(bind_addr = %addr, "Starting TaskRun control plane");

    // Create shared state
    let state = AppState::new();

    // Create services
    let run_service = RunServiceImpl::new(state.clone()).into_server();
    let task_service = TaskServiceImpl::new(state.clone()).into_server();
    let worker_service = WorkerServiceImpl::new(state).into_server();

    // Start server
    Server::builder()
        .add_service(run_service)
        .add_service(task_service)
        .add_service(worker_service)
        .serve(addr)
        .await?;

    Ok(())
}
