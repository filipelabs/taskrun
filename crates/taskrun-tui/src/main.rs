//! TaskRun Terminal UI.
//!
//! Provides terminal-based dashboards for monitoring TaskRun control plane and workers.

use std::error::Error;
use std::time::Duration;

use clap::{Parser, Subcommand};
use tokio::sync::mpsc;
use tracing::info;

mod app;
mod backend;
mod event;
mod state;
mod ui;
mod worker;

use app::App;
use event::{BackendCommand, UiEvent};

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
        #[arg(short, long, default_value = "https://[::1]:50051")]
        endpoint: String,

        /// Control plane HTTP endpoint (for REST API)
        #[arg(long, default_value = "http://[::1]:50052")]
        http_endpoint: String,

        /// CA certificate for TLS (PEM file path)
        #[arg(long)]
        ca_cert: Option<String>,

        /// Client certificate for mTLS (PEM file path)
        #[arg(long)]
        client_cert: Option<String>,

        /// Client key for mTLS (PEM file path)
        #[arg(long)]
        client_key: Option<String>,

        /// Refresh interval in seconds
        #[arg(short, long, default_value = "2")]
        refresh: u64,
    },

    /// Worker TUI - run a worker with interactive dashboard
    #[command(alias = "w")]
    Worker {
        /// Agent name to run (e.g., general, support_triage)
        #[arg(short, long, default_value = "general")]
        agent: String,

        /// Model to use (opus, sonnet, haiku, or full name like claude-sonnet-4-5)
        #[arg(short, long, default_value = "claude-sonnet-4-5")]
        model: String,

        /// Control plane gRPC endpoint
        #[arg(short, long, default_value = "https://[::1]:50051")]
        endpoint: String,

        /// CA certificate for TLS (PEM file path)
        #[arg(long, default_value = "certs/ca.crt")]
        ca_cert: String,

        /// Client certificate for mTLS (PEM file path)
        #[arg(long, default_value = "certs/worker.crt")]
        client_cert: String,

        /// Client key for mTLS (PEM file path)
        #[arg(long, default_value = "certs/worker.key")]
        client_key: String,

        /// Tools to allow (comma-separated, e.g., "Read,Write,Bash")
        #[arg(long)]
        allow_tools: Option<String>,

        /// Tools to deny (comma-separated, e.g., "WebSearch,Bash")
        #[arg(long)]
        deny_tools: Option<String>,

        /// Maximum concurrent runs
        #[arg(long, default_value = "10")]
        max_concurrent_runs: u32,

        /// Working directory for the agent
        #[arg(short = 'd', long, default_value = ".")]
        working_dir: String,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing - write to file to avoid terminal interference
    // Logs go to /tmp/taskrun-tui.log
    let log_file = std::fs::File::create("/tmp/taskrun-tui.log").ok();
    if let Some(file) = log_file {
        tracing_subscriber::fmt()
            .with_writer(std::sync::Mutex::new(file))
            .with_env_filter("taskrun_tui=debug")
            .with_ansi(false)
            .init();
    }

    let cli = Cli::parse();

    match cli.command {
        Commands::ControlPlane {
            endpoint,
            http_endpoint: _,
            ca_cert,
            client_cert,
            client_key,
            refresh,
        } => {
            run_control_plane_tui(
                &endpoint,
                ca_cert.as_deref(),
                client_cert.as_deref(),
                client_key.as_deref(),
                refresh,
            )?;
        }
        Commands::Worker {
            agent,
            model,
            endpoint,
            ca_cert,
            client_cert,
            client_key,
            allow_tools,
            deny_tools,
            max_concurrent_runs,
            working_dir,
        } => {
            // Resolve working directory to absolute path
            let working_dir = std::fs::canonicalize(&working_dir)
                .unwrap_or_else(|_| std::path::PathBuf::from(&working_dir))
                .to_string_lossy()
                .to_string();

            let config = worker::WorkerConfig {
                agent_name: agent,
                model_name: model,
                endpoint,
                ca_cert_path: ca_cert,
                client_cert_path: client_cert,
                client_key_path: client_key,
                allowed_tools: allow_tools.map(|s| parse_tools(&s)),
                denied_tools: deny_tools.map(|s| parse_tools(&s)),
                max_concurrent_runs,
                working_dir,
                skip_permissions: true, // Default to true, can be changed in setup UI
            };
            worker::run_worker_tui(config)?;
        }
    }

    Ok(())
}

fn run_control_plane_tui(
    grpc_endpoint: &str,
    ca_cert: Option<&str>,
    client_cert: Option<&str>,
    client_key: Option<&str>,
    refresh: u64,
) -> Result<(), Box<dyn Error>> {
    info!(endpoint = %grpc_endpoint, refresh = refresh, "Starting control plane TUI");

    // Read CA cert if provided
    let ca_cert_bytes = if let Some(path) = ca_cert {
        Some(std::fs::read(path)?)
    } else {
        None
    };

    // Read client cert/key for mTLS if provided
    let client_identity = match (client_cert, client_key) {
        (Some(cert_path), Some(key_path)) => {
            let cert = std::fs::read(cert_path)?;
            let key = std::fs::read(key_path)?;
            Some((cert, key))
        }
        _ => None,
    };

    // Create channels for UI <-> backend communication
    let (ui_tx, ui_rx) = mpsc::channel::<UiEvent>(100);
    let (cmd_tx, cmd_rx) = mpsc::channel::<BackendCommand>(100);

    // Spawn background thread with its own tokio runtime
    let endpoint = grpc_endpoint.to_string();
    let refresh_duration = Duration::from_secs(refresh);
    let bg_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(backend::run_backend(
            endpoint,
            ca_cert_bytes,
            client_identity,
            refresh_duration,
            ui_tx,
            cmd_rx,
        ));
    });

    // Initialize terminal (enters alternate screen, enables raw mode)
    let terminal = ratatui::init();

    // Run UI loop on main thread
    let mut app = App::new(ui_rx, cmd_tx);
    let result = app.run(terminal);

    // Restore terminal (exits alternate screen, disables raw mode)
    ratatui::restore();

    // Wait for background thread to finish
    let _ = bg_handle.join();

    info!("TUI shutdown complete");

    result.map_err(|e| e.into())
}

/// Parse comma-separated tool names into a vector.
fn parse_tools(tools: &str) -> Vec<String> {
    tools
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
