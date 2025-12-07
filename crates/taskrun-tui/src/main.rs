//! TaskRun Terminal UI.
//!
//! Provides terminal-based dashboards for monitoring TaskRun control plane and workers.

use clap::{Parser, Subcommand};
use std::error::Error;

mod app;
mod event;
mod ui;

#[derive(Parser)]
#[command(name = "taskrun-tui")]
#[command(about = "TaskRun Terminal UI")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Control plane dashboard - view workers, tasks, runs, and agent traces
    #[command(alias = "cp")]
    ControlPlane {
        /// Control plane gRPC endpoint
        #[arg(short, long, default_value = "http://[::1]:50051")]
        endpoint: String,

        /// Control plane HTTP endpoint (for REST API)
        #[arg(long, default_value = "http://[::1]:50052")]
        http_endpoint: String,

        /// CA certificate for TLS (PEM file path)
        #[arg(long)]
        ca_cert: Option<String>,

        /// Refresh interval in seconds
        #[arg(short, long, default_value = "2")]
        refresh: u64,
    },

    /// Worker local dashboard - view connection status, active runs, and output
    Worker {
        /// Worker admin endpoint
        #[arg(short, long, default_value = "http://127.0.0.1:50060")]
        endpoint: String,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing (logs to file to avoid terminal interference)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("taskrun_tui=debug")
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::ControlPlane {
            endpoint,
            http_endpoint,
            ca_cert,
            refresh,
        } => {
            run_control_plane_tui(&endpoint, &http_endpoint, ca_cert.as_deref(), refresh)?;
        }
        Commands::Worker { endpoint } => {
            run_worker_tui(&endpoint)?;
        }
    }

    Ok(())
}

fn run_control_plane_tui(
    _grpc_endpoint: &str,
    _http_endpoint: &str,
    _ca_cert: Option<&str>,
    _refresh: u64,
) -> Result<(), Box<dyn Error>> {
    // Initialize terminal
    let terminal = ratatui::init();

    // Run app
    let mut app = app::App::new();
    let result = app.run(terminal);

    // Restore terminal
    ratatui::restore();

    result.map_err(|e| e.into())
}

fn run_worker_tui(_endpoint: &str) -> Result<(), Box<dyn Error>> {
    // Initialize terminal
    let terminal = ratatui::init();

    // Run app
    let mut app = app::App::new();
    let result = app.run(terminal);

    // Restore terminal
    ratatui::restore();

    result.map_err(|e| e.into())
}
