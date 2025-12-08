//! Worker TUI state types.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

/// Worker configuration from CLI arguments.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub agent_name: String,
    pub model_name: String,
    pub endpoint: String,
    pub ca_cert_path: String,
    pub client_cert_path: String,
    pub client_key_path: String,
    pub allowed_tools: Option<Vec<String>>,
    pub denied_tools: Option<Vec<String>>,
    pub max_concurrent_runs: u32,
    pub working_dir: String,
}

impl WorkerConfig {
    /// Parse model string into (provider, model_name).
    pub fn parse_model(&self) -> (String, String) {
        // Check for provider prefix
        if let Some((provider, model_name)) = self.model_name.split_once('/') {
            return (provider.to_string(), model_name.to_string());
        }

        // Map short names to full names
        let model_name = match self.model_name.to_lowercase().as_str() {
            "opus" => "claude-opus-4-5",
            "sonnet" => "claude-sonnet-4-5",
            "haiku" => "claude-haiku-4-5",
            _ => &self.model_name,
        };

        ("anthropic".to_string(), model_name.to_string())
    }
}

/// Available views in the worker TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerView {
    Status,
    Runs,
    Logs,
    Config,
}

impl WorkerView {
    pub fn all() -> &'static [WorkerView] {
        &[
            WorkerView::Status,
            WorkerView::Runs,
            WorkerView::Logs,
            WorkerView::Config,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            WorkerView::Status => "Status",
            WorkerView::Runs => "Runs",
            WorkerView::Logs => "Logs",
            WorkerView::Config => "Config",
        }
    }

    pub fn next(&self) -> WorkerView {
        match self {
            WorkerView::Status => WorkerView::Runs,
            WorkerView::Runs => WorkerView::Logs,
            WorkerView::Logs => WorkerView::Config,
            WorkerView::Config => WorkerView::Status,
        }
    }

    pub fn prev(&self) -> WorkerView {
        match self {
            WorkerView::Status => WorkerView::Config,
            WorkerView::Runs => WorkerView::Status,
            WorkerView::Logs => WorkerView::Runs,
            WorkerView::Config => WorkerView::Logs,
        }
    }
}

/// Connection state for the worker.
#[derive(Debug, Clone)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected { retry_in: Duration },
}

/// Status of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
}

/// Information about a run.
#[derive(Debug, Clone)]
pub struct RunInfo {
    pub run_id: String,
    pub task_id: String,
    pub agent: String,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub output_preview: String,
}

/// Log entry for the logs view.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Debug => "DEBUG",
        }
    }
}

/// Worker statistics.
#[derive(Debug, Clone, Default)]
pub struct WorkerStats {
    pub total_runs: u64,
    pub successful_runs: u64,
    pub failed_runs: u64,
}

/// Main UI state for the worker TUI.
pub struct WorkerUiState {
    pub config: WorkerConfig,
    pub worker_id: String,
    pub connection_state: ConnectionState,
    pub current_view: WorkerView,
    pub active_runs: Vec<RunInfo>,
    pub completed_runs: VecDeque<RunInfo>,
    pub log_messages: VecDeque<LogEntry>,
    pub stats: WorkerStats,
    pub start_time: Instant,
    pub selected_run_index: usize,
    pub log_scroll_offset: usize,
    pub status_message: Option<String>,
}

impl WorkerUiState {
    pub fn new(config: WorkerConfig, worker_id: String) -> Self {
        Self {
            config,
            worker_id,
            connection_state: ConnectionState::Connecting,
            current_view: WorkerView::Status,
            active_runs: Vec::new(),
            completed_runs: VecDeque::with_capacity(100),
            log_messages: VecDeque::with_capacity(1000),
            stats: WorkerStats::default(),
            start_time: Instant::now(),
            selected_run_index: 0,
            log_scroll_offset: 0,
            status_message: None,
        }
    }

    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn add_log(&mut self, level: LogLevel, message: String) {
        self.log_messages.push_back(LogEntry {
            timestamp: Utc::now(),
            level,
            message,
        });
        // Keep only last 1000 messages
        while self.log_messages.len() > 1000 {
            self.log_messages.pop_front();
        }
    }

    pub fn add_run(&mut self, run: RunInfo) {
        self.active_runs.push(run);
    }

    pub fn complete_run(&mut self, run_id: &str, success: bool) {
        if let Some(pos) = self.active_runs.iter().position(|r| r.run_id == run_id) {
            let mut run = self.active_runs.remove(pos);
            run.status = if success {
                RunStatus::Completed
            } else {
                RunStatus::Failed
            };
            run.completed_at = Some(Utc::now());

            // Update stats
            self.stats.total_runs += 1;
            if success {
                self.stats.successful_runs += 1;
            } else {
                self.stats.failed_runs += 1;
            }

            // Add to completed runs (keep last 100)
            self.completed_runs.push_front(run);
            while self.completed_runs.len() > 100 {
                self.completed_runs.pop_back();
            }
        }
    }
}
