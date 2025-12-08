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
    RunDetail,
    Logs,
    Config,
}

impl WorkerView {
    /// Views shown in the tab bar (excludes RunDetail which is a drill-down).
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
            WorkerView::RunDetail => "Run Detail",
            WorkerView::Logs => "Logs",
            WorkerView::Config => "Config",
        }
    }

    pub fn next(&self) -> WorkerView {
        match self {
            WorkerView::Status => WorkerView::Runs,
            WorkerView::Runs => WorkerView::Logs,
            WorkerView::RunDetail => WorkerView::Logs,
            WorkerView::Logs => WorkerView::Config,
            WorkerView::Config => WorkerView::Status,
        }
    }

    pub fn prev(&self) -> WorkerView {
        match self {
            WorkerView::Status => WorkerView::Config,
            WorkerView::Runs => WorkerView::Status,
            WorkerView::RunDetail => WorkerView::Runs,
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

/// Event that occurred during a run.
#[derive(Debug, Clone)]
pub struct RunEventInfo {
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub details: Option<String>,
}

/// Role in a chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Assistant,
}

/// A message in the chat history.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

impl ChatMessage {
    pub fn user(content: String) -> Self {
        Self {
            role: ChatRole::User,
            content,
            timestamp: Utc::now(),
        }
    }

    pub fn assistant(content: String) -> Self {
        Self {
            role: ChatRole::Assistant,
            content,
            timestamp: Utc::now(),
        }
    }
}

/// Maximum output size per run (50KB).
const MAX_OUTPUT_SIZE: usize = 50_000;

/// Maximum events per run.
const MAX_EVENTS_PER_RUN: usize = 100;

/// Maximum messages in chat history.
const MAX_CHAT_MESSAGES: usize = 100;

/// Information about a run.
#[derive(Debug, Clone)]
pub struct RunInfo {
    pub run_id: String,
    pub task_id: String,
    pub agent: String,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub messages: Vec<ChatMessage>,
    pub current_output: String,
    pub events: Vec<RunEventInfo>,
    pub queued_input: Option<String>,
}

impl RunInfo {
    /// Create a new RunInfo with the initial user message.
    pub fn new(run_id: String, task_id: String, agent: String, input: String) -> Self {
        let mut messages = Vec::new();
        messages.push(ChatMessage::user(input));
        Self {
            run_id,
            task_id,
            agent,
            status: RunStatus::Running,
            started_at: Utc::now(),
            completed_at: None,
            messages,
            current_output: String::new(),
            events: Vec::new(),
            queued_input: None,
        }
    }

    /// Append to current streaming output.
    pub fn append_output(&mut self, text: &str) {
        self.current_output.push_str(text);
        // Trim from front if too large
        if self.current_output.len() > MAX_OUTPUT_SIZE {
            let excess = self.current_output.len() - MAX_OUTPUT_SIZE;
            self.current_output = self.current_output[excess..].to_string();
        }
    }

    /// Finalize current output as an assistant message.
    pub fn finalize_output(&mut self) {
        if !self.current_output.is_empty() {
            if self.messages.len() >= MAX_CHAT_MESSAGES {
                self.messages.remove(0);
            }
            self.messages.push(ChatMessage::assistant(self.current_output.clone()));
            self.current_output.clear();
        }
    }

    /// Add a new user message (for continuation).
    pub fn add_user_message(&mut self, content: String) {
        if self.messages.len() >= MAX_CHAT_MESSAGES {
            self.messages.remove(0);
        }
        self.messages.push(ChatMessage::user(content));
    }

    /// Add an event, keeping under the limit.
    pub fn add_event(&mut self, event_type: String, details: Option<String>) {
        if self.events.len() >= MAX_EVENTS_PER_RUN {
            self.events.remove(0);
        }
        self.events.push(RunEventInfo {
            timestamp: Utc::now(),
            event_type,
            details,
        });
    }

    /// Get the initial input (first user message).
    pub fn initial_input(&self) -> &str {
        self.messages
            .first()
            .map(|m| m.content.as_str())
            .unwrap_or("")
    }
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

/// Which pane is focused in the run detail view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailPane {
    #[default]
    Output,
    Events,
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
    // Run detail/chat view state
    pub viewing_run_id: Option<String>,
    pub detail_pane: DetailPane,
    pub chat_scroll: usize,
    pub events_scroll: usize,
    // Chat input state
    pub chat_input: String,
    pub chat_input_cursor: usize,
    pub input_focused: bool,
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
            viewing_run_id: None,
            detail_pane: DetailPane::default(),
            chat_scroll: 0,
            events_scroll: 0,
            chat_input: String::new(),
            chat_input_cursor: 0,
            input_focused: true,
        }
    }

    /// Get the currently viewing run (from active or completed).
    pub fn get_viewing_run(&self) -> Option<&RunInfo> {
        let run_id = self.viewing_run_id.as_ref()?;
        self.active_runs
            .iter()
            .find(|r| &r.run_id == run_id)
            .or_else(|| self.completed_runs.iter().find(|r| &r.run_id == run_id))
    }

    /// Get the currently selected run (from combined list).
    pub fn get_selected_run(&self) -> Option<&RunInfo> {
        let all_runs: Vec<_> = self.active_runs.iter().chain(self.completed_runs.iter()).collect();
        all_runs.get(self.selected_run_index).copied()
    }

    /// Enter detail view for the selected run.
    pub fn enter_run_detail(&mut self) {
        if let Some(run) = self.get_selected_run() {
            self.viewing_run_id = Some(run.run_id.clone());
            self.current_view = WorkerView::RunDetail;
            self.detail_pane = DetailPane::Output;
            self.chat_scroll = 0;
            self.events_scroll = 0;
            self.chat_input.clear();
            self.chat_input_cursor = 0;
            self.input_focused = true;
        }
    }

    /// Exit detail view and return to runs list.
    pub fn exit_run_detail(&mut self) {
        self.viewing_run_id = None;
        self.current_view = WorkerView::Runs;
        self.chat_input.clear();
    }

    /// Get mutable reference to viewing run.
    pub fn get_viewing_run_mut(&mut self) -> Option<&mut RunInfo> {
        let run_id = self.viewing_run_id.clone()?;
        self.active_runs
            .iter_mut()
            .find(|r| r.run_id == run_id)
            .or_else(|| {
                self.completed_runs
                    .iter_mut()
                    .find(|r| r.run_id == run_id)
            })
    }

    /// Queue a message for the current run.
    pub fn queue_chat_message(&mut self) {
        if self.chat_input.is_empty() {
            return;
        }
        // Clone input first to avoid borrow conflict
        let input = self.chat_input.clone();
        if let Some(run) = self.get_viewing_run_mut() {
            run.queued_input = Some(input);
        }
        self.chat_input.clear();
        self.chat_input_cursor = 0;
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
