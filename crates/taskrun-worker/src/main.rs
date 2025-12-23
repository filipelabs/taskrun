//! TaskRun Worker Daemon

use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod config;
mod connection;
mod executor;
mod json_output;

#[cfg(feature = "tui")]
mod tui;

use config::{Cli, Config};
use connection::WorkerConnection;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI arguments
    let cli = Cli::parse();

    // --json implies headless mode
    if cli.json {
        return run_json_mode(cli);
    }

    // Run headless if requested or if TUI feature is not available
    if cli.headless {
        return run_headless_mode(cli);
    }

    // Default: run TUI mode
    #[cfg(feature = "tui")]
    {
        run_tui_mode(cli)
    }

    #[cfg(not(feature = "tui"))]
    {
        eprintln!("TUI not available. Rebuild with: cargo build -p taskrun-worker --features tui");
        eprintln!("Or run with --headless flag for daemon mode.");
        std::process::exit(1);
    }
}

/// Run the worker in headless mode (daemon).
fn run_headless_mode(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
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

    // Create tokio runtime and run
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        // Reconnection loop
        loop {
            let mut connection = WorkerConnection::new(config.clone());

            match connection.connect_and_run().await {
                Ok(_) => {
                    info!("Connection closed normally");
                }
                Err(e) => {
                    // Extract root cause from error chain
                    let root_cause = get_root_cause(&*e);
                    error!(error = %root_cause, "Connection failed");
                }
            }

            info!(
                delay_secs = config.reconnect_delay_secs,
                "Reconnecting in {} seconds...", config.reconnect_delay_secs
            );
            tokio::time::sleep(Duration::from_secs(config.reconnect_delay_secs)).await;
        }
    })
}

/// Run the worker in JSON mode (headless with JSON line output to stdout).
fn run_json_mode(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Enable JSON output mode
    json_output::enable_json_mode();

    // Initialize tracing with log level from CLI, output to stderr
    let filter = EnvFilter::try_new(&cli.log_level).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_writer(std::io::stderr)
        .init();

    // Build config from CLI
    let config = Arc::new(Config::from_cli(&cli));

    info!(
        worker_id = %config.worker_id,
        control_plane = %config.control_plane_addr,
        agent = %config.agent_name,
        model = format!("{}/{}", config.model_provider, config.model_name),
        json_mode = true,
        "Starting TaskRun worker in JSON mode"
    );

    // Create tokio runtime and run
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        // Reconnection loop
        loop {
            let mut connection = WorkerConnection::new(config.clone());

            match connection.connect_and_run().await {
                Ok(_) => {
                    info!("Connection closed normally");
                    json_output::emit_worker_disconnected(
                        config.worker_id.as_str(),
                        Some("Connection closed normally"),
                    );
                }
                Err(e) => {
                    error!(error = %e, "Connection error");
                    json_output::emit_error(&format!("Connection error: {}", e), None);
                }
            }

            info!(
                delay_secs = config.reconnect_delay_secs,
                "Reconnecting in {} seconds...", config.reconnect_delay_secs
            );
            tokio::time::sleep(Duration::from_secs(config.reconnect_delay_secs)).await;
        }
    })
}

/// Run the worker in TUI mode (interactive terminal UI).
#[cfg(feature = "tui")]
fn run_tui_mode(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve working directory to absolute path
    let working_dir = std::fs::canonicalize(&cli.working_dir)
        .unwrap_or_else(|_| std::path::PathBuf::from(&cli.working_dir))
        .to_string_lossy()
        .to_string();

    let config = tui::WorkerConfig {
        agent_name: cli.agent,
        model_name: cli.model,
        endpoint: cli.endpoint,
        ca_cert_path: cli.ca_cert,
        client_cert_path: cli.client_cert,
        client_key_path: cli.client_key,
        allowed_tools: cli.allow_tools.map(|s| parse_tools(&s)),
        denied_tools: cli.deny_tools.map(|s| parse_tools(&s)),
        max_concurrent_runs: cli.max_concurrent_runs,
        working_dir,
        skip_permissions: true,
    };

    tui::run_worker_tui(config)
}

/// Parse comma-separated tool names into a vector.
#[cfg(feature = "tui")]
fn parse_tools(tools: &str) -> Vec<String> {
    tools
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Extract a user-friendly error message from an error chain.
/// Looks for common TLS/connection errors and provides actionable messages.
pub fn get_root_cause(err: &(dyn std::error::Error + 'static)) -> String {
    let mut current: &dyn std::error::Error = err;
    let mut messages = Vec::new();

    loop {
        let msg = current.to_string();
        messages.push(msg.clone());

        // Check for specific TLS errors and provide helpful messages
        if msg.contains("CertificateExpired") {
            return "Certificate expired. Run: scripts/gen-worker-cert.sh".to_string();
        }
        if msg.contains("CertificateRequired") {
            return "Server requires client certificate. Check --client-cert and --client-key".to_string();
        }
        if msg.contains("CertificateUnknown") || msg.contains("UnknownCA") {
            return "Certificate not trusted. Check --ca-cert matches server's CA".to_string();
        }
        if msg.contains("HandshakeFailure") {
            return "TLS handshake failed. Check certificate configuration".to_string();
        }
        if msg.contains("Connection refused") {
            return "Connection refused. Is the server running?".to_string();
        }

        match current.source() {
            Some(source) => current = source,
            None => break,
        }
    }

    // Return the deepest error message if no specific match
    messages.pop().unwrap_or_else(|| err.to_string())
}
