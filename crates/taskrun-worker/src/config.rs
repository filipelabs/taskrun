//! Worker configuration.

use clap::Parser;
use taskrun_core::WorkerId;

/// CLI arguments for the worker.
#[derive(Parser)]
#[command(name = "taskrun-worker")]
#[command(about = "TaskRun Worker - connects to control plane and executes agent tasks")]
#[command(version)]
pub struct Cli {
    /// Run in headless mode (daemon without TUI)
    #[arg(long)]
    pub headless: bool,

    /// Agent name to run (e.g., general, support_triage)
    #[arg(short, long, default_value = "general")]
    pub agent: String,

    /// Model to use (e.g., claude-opus-4-5, claude-sonnet-4-5, claude-haiku-4-5)
    #[arg(short, long, default_value = "claude-sonnet-4-5")]
    pub model: String,

    /// Control plane gRPC endpoint
    #[arg(short, long, default_value = "https://[::1]:50051")]
    pub endpoint: String,

    /// CA certificate for TLS (PEM file path)
    #[arg(long, default_value = "certs/ca.crt")]
    pub ca_cert: String,

    /// Client certificate for mTLS (PEM file path)
    #[arg(long, default_value = "certs/worker.crt")]
    pub client_cert: String,

    /// Client key for mTLS (PEM file path)
    #[arg(long, default_value = "certs/worker.key")]
    pub client_key: String,

    /// Tools to allow (comma-separated, e.g., "Read,Write,Bash")
    #[arg(long)]
    pub allow_tools: Option<String>,

    /// Tools to deny (comma-separated, e.g., "WebSearch,Bash")
    #[arg(long)]
    pub deny_tools: Option<String>,

    /// Log level (e.g., info, debug, warn)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Heartbeat interval in seconds
    #[arg(long, default_value = "15")]
    pub heartbeat_interval: u64,

    /// Maximum concurrent runs
    #[arg(long, default_value = "10")]
    pub max_concurrent_runs: u32,

    /// Working directory for agent execution (TUI mode)
    #[arg(short = 'd', long, default_value = ".")]
    pub working_dir: String,
}

/// Worker configuration.
pub struct Config {
    /// Control plane address (must be https:// for TLS).
    pub control_plane_addr: String,

    /// Worker ID.
    pub worker_id: WorkerId,

    /// Heartbeat interval in seconds.
    pub heartbeat_interval_secs: u64,

    /// Reconnection delay on connection loss (seconds).
    pub reconnect_delay_secs: u64,

    /// Maximum concurrent runs this worker can handle.
    pub max_concurrent_runs: u32,

    /// Path to CA certificate for verifying control plane (CA pinning).
    pub tls_ca_cert_path: String,

    /// Path to worker client certificate for mTLS (PEM format).
    pub tls_cert_path: String,

    /// Path to worker client private key for mTLS (PEM format).
    pub tls_key_path: String,

    /// Path to Claude Code CLI binary.
    pub claude_path: String,

    // Agent configuration
    /// Agent name to advertise and handle.
    pub agent_name: String,

    /// Model provider (e.g., "anthropic").
    pub model_provider: String,

    /// Model name (e.g., "claude-opus-4-5").
    pub model_name: String,

    /// Tools to allow (if specified).
    pub allowed_tools: Option<Vec<String>>,

    /// Tools to deny (if specified).
    pub denied_tools: Option<Vec<String>>,
}

impl Config {
    /// Create a Config from CLI arguments.
    pub fn from_cli(cli: &Cli) -> Self {
        let (provider, model) = parse_model_string(&cli.model);

        Self {
            control_plane_addr: cli.endpoint.clone(),
            worker_id: WorkerId::generate(),
            heartbeat_interval_secs: cli.heartbeat_interval,
            reconnect_delay_secs: 5,
            max_concurrent_runs: cli.max_concurrent_runs,
            tls_ca_cert_path: cli.ca_cert.clone(),
            tls_cert_path: cli.client_cert.clone(),
            tls_key_path: cli.client_key.clone(),
            claude_path: "claude".to_string(),
            agent_name: cli.agent.clone(),
            model_provider: provider,
            model_name: model,
            allowed_tools: cli.allow_tools.as_ref().map(|s| parse_tools(s)),
            denied_tools: cli.deny_tools.as_ref().map(|s| parse_tools(s)),
        }
    }
}

/// Parse a model string into (provider, model_name).
///
/// Supports:
/// - Full names: "claude-opus-4-5", "claude-sonnet-4-5", "claude-haiku-4-5"
/// - Short names: "opus", "sonnet", "haiku"
/// - Provider prefix: "anthropic/claude-opus-4-5"
fn parse_model_string(model: &str) -> (String, String) {
    // Check for provider prefix
    if let Some((provider, model_name)) = model.split_once('/') {
        return (provider.to_string(), model_name.to_string());
    }

    // Map short names to full names
    let model_name = match model.to_lowercase().as_str() {
        "opus" => "claude-opus-4-5",
        "sonnet" => "claude-sonnet-4-5",
        "haiku" => "claude-haiku-4-5",
        _ => model,
    };

    // Default provider is anthropic
    ("anthropic".to_string(), model_name.to_string())
}

/// Parse a comma-separated list of tools.
fn parse_tools(tools: &str) -> Vec<String> {
    tools
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            control_plane_addr: "https://[::1]:50051".to_string(),
            worker_id: WorkerId::generate(),
            heartbeat_interval_secs: 15,
            reconnect_delay_secs: 5,
            max_concurrent_runs: 10,
            tls_ca_cert_path: "certs/ca.crt".to_string(),
            tls_cert_path: "certs/worker.crt".to_string(),
            tls_key_path: "certs/worker.key".to_string(),
            claude_path: "claude".to_string(),
            agent_name: "general".to_string(),
            model_provider: "anthropic".to_string(),
            model_name: "claude-sonnet-4-5".to_string(),
            allowed_tools: None,
            denied_tools: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_model_string_full_name() {
        let (provider, model) = parse_model_string("claude-opus-4-5");
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-opus-4-5");
    }

    #[test]
    fn test_parse_model_string_short_name() {
        let (provider, model) = parse_model_string("opus");
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-opus-4-5");

        let (provider, model) = parse_model_string("sonnet");
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-sonnet-4-5");

        let (provider, model) = parse_model_string("haiku");
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-haiku-4-5");
    }

    #[test]
    fn test_parse_model_string_with_provider() {
        let (provider, model) = parse_model_string("anthropic/claude-opus-4-5");
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-opus-4-5");
    }

    #[test]
    fn test_parse_tools() {
        let tools = parse_tools("Read,Write,Bash");
        assert_eq!(tools, vec!["Read", "Write", "Bash"]);

        let tools = parse_tools("Read, Write , Bash");
        assert_eq!(tools, vec!["Read", "Write", "Bash"]);

        let tools = parse_tools("");
        assert!(tools.is_empty());
    }
}
