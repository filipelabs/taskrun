//! TaskRun Server with TUI.
//!
//! Control plane server with terminal user interface for monitoring and management.

mod app;
mod backend;
mod event;
mod render;
mod state;
mod views;

use std::io::{self, stdout};
use std::thread;

use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;
use tracing::info;
use tracing_subscriber::EnvFilter;

use app::ServerApp;
use backend::{ServerConfig, run_server_backend};
use event::{ServerCommand, ServerUiEvent};

/// TaskRun control plane server with TUI.
#[derive(Parser, Debug)]
#[command(name = "taskrun-server", about = "TaskRun control plane server with TUI")]
struct Args {
    /// gRPC server address
    #[arg(long, default_value = "[::1]:50051")]
    grpc_addr: String,

    /// HTTP server address
    #[arg(long, default_value = "[::1]:50052")]
    http_addr: String,

    /// Path to server TLS certificate
    #[arg(long, default_value = "certs/server.crt")]
    tls_cert: String,

    /// Path to server TLS key
    #[arg(long, default_value = "certs/server.key")]
    tls_key: String,

    /// Path to CA certificate
    #[arg(long, default_value = "certs/ca.crt")]
    ca_cert: String,

    /// Path to CA private key (for worker enrollment)
    #[arg(long, default_value = "certs/ca.key")]
    ca_key: String,

    /// Worker certificate validity in days
    #[arg(long, default_value = "7")]
    worker_cert_validity_days: u32,
}

fn main() -> io::Result<()> {
    // Parse CLI arguments
    let args = Args::parse();

    // Initialize logging (to file, not stderr since we have TUI)
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("taskrun=info".parse().unwrap()))
        .with_writer(|| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("taskrun-server.log")
                .unwrap_or_else(|_| std::fs::File::create("/dev/null").unwrap())
        })
        .init();

    info!("TaskRun Server starting");

    // Create channels for UI <-> backend communication
    let (ui_tx, ui_rx) = mpsc::channel::<ServerUiEvent>(1000);
    let (cmd_tx, cmd_rx) = mpsc::channel::<ServerCommand>(100);

    // Build server config
    let config = ServerConfig {
        grpc_addr: args.grpc_addr,
        http_addr: args.http_addr,
        tls_cert_path: args.tls_cert,
        tls_key_path: args.tls_key,
        ca_cert_path: args.ca_cert,
        ca_key_path: args.ca_key,
        worker_cert_validity_days: args.worker_cert_validity_days,
    };

    // Spawn backend in a separate thread with its own tokio runtime
    let backend_handle = thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(run_server_backend(config, ui_tx, cmd_rx));
    });

    // Setup terminal
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    // Run the TUI app
    let result = ServerApp::new(ui_rx, cmd_tx).run(&mut terminal);

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // Wait for backend to finish (it should exit when it receives Shutdown command)
    let _ = backend_handle.join();

    info!("TaskRun Server stopped");

    result
}
