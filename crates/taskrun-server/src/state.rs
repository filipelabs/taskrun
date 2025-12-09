//! Server TUI state types.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use chrono::{DateTime, Utc};
use taskrun_core::{ChatRole, RunEventType, RunId, RunStatus, TaskId, TaskStatus, WorkerId, WorkerStatus};
use taskrun_tui_components::{LogEntry, LogLevel};

/// Server views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerView {
    Workers,
    Tasks,
    Logs,
    RunDetail,
}

impl ServerView {
    /// Views shown in the tab bar (excludes RunDetail which is a drill-down).
    pub fn all() -> &'static [ServerView] {
        &[ServerView::Workers, ServerView::Tasks, ServerView::Logs]
    }

    pub fn name(&self) -> &'static str {
        match self {
            ServerView::Workers => "Workers",
            ServerView::Tasks => "Tasks",
            ServerView::Logs => "Logs",
            ServerView::RunDetail => "Run Detail",
        }
    }

    pub fn next(&self) -> ServerView {
        match self {
            ServerView::Workers => ServerView::Tasks,
            ServerView::Tasks => ServerView::Logs,
            ServerView::Logs => ServerView::Workers,
            ServerView::RunDetail => ServerView::Tasks,
        }
    }

    pub fn prev(&self) -> ServerView {
        match self {
            ServerView::Workers => ServerView::Logs,
            ServerView::Tasks => ServerView::Workers,
            ServerView::Logs => ServerView::Tasks,
            ServerView::RunDetail => ServerView::Tasks,
        }
    }
}

/// Cached worker info for display.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields for future display use
pub struct WorkerDisplayInfo {
    pub worker_id: WorkerId,
    pub hostname: String,
    pub agents: Vec<String>,
    pub status: WorkerStatus,
    pub active_runs: u32,
    pub max_concurrent_runs: u32,
    pub connected_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
}

/// Cached task info for display.
#[derive(Debug, Clone)]
pub struct TaskDisplayInfo {
    pub task_id: TaskId,
    pub agent_name: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub run_count: usize,
    pub latest_run_id: Option<RunId>,
    pub latest_run_status: Option<RunStatus>,
}

/// Chat message entry for display.
#[derive(Debug, Clone)]
#[allow(dead_code)] // timestamp for future display use
pub struct ChatEntry {
    pub timestamp: DateTime<Utc>,
    pub role: ChatRole,
    pub content: String,
}

/// Run event entry for display.
#[derive(Debug, Clone)]
pub struct EventEntry {
    pub timestamp: DateTime<Utc>,
    pub event_type: RunEventType,
    pub details: Option<String>,
}

/// Server status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerStatus {
    Starting,
    Running,
    Error,
}

/// Main UI state.
pub struct ServerUiState {
    // Server info
    pub server_status: ServerStatus,
    pub grpc_addr: String,
    pub http_addr: String,
    pub start_time: Instant,
    pub error_message: Option<String>,

    // View navigation
    pub current_view: ServerView,

    // Workers view
    pub workers: HashMap<WorkerId, WorkerDisplayInfo>,
    pub selected_worker_index: usize,

    // Tasks view
    pub tasks: HashMap<TaskId, TaskDisplayInfo>,
    pub task_list: Vec<TaskId>, // Sorted list for display
    pub selected_task_index: usize,

    // Run detail view
    pub viewing_task_id: Option<TaskId>,
    pub run_output: HashMap<RunId, String>,
    pub run_chat: HashMap<RunId, Vec<ChatEntry>>,     // Chat messages per run
    pub run_events: HashMap<RunId, Vec<EventEntry>>,  // Events per run
    pub run_scroll: usize,
    pub events_scroll: usize,
    pub chat_input: String,       // Current chat input text
    pub chat_input_cursor: usize, // Cursor position in chat input

    // Logs view
    pub log_messages: VecDeque<LogEntry>,
    pub log_scroll: usize,

    // Dialogs
    pub show_new_task_dialog: bool,
    pub new_task_agent: String,
    pub new_task_input: String,
    pub new_task_cursor: usize,
    pub new_task_field: usize, // 0 = agent, 1 = input

    pub show_cancel_confirm: bool,
    pub show_disconnect_confirm: bool,
    pub show_quit_confirm: bool,

    // Stats
    pub total_tasks: u64,
    pub completed_tasks: u64,
    pub failed_tasks: u64,
}

impl ServerUiState {
    pub fn new() -> Self {
        Self {
            server_status: ServerStatus::Starting,
            grpc_addr: String::new(),
            http_addr: String::new(),
            start_time: Instant::now(),
            error_message: None,

            current_view: ServerView::Workers,

            workers: HashMap::new(),
            selected_worker_index: 0,

            tasks: HashMap::new(),
            task_list: Vec::new(),
            selected_task_index: 0,

            viewing_task_id: None,
            run_output: HashMap::new(),
            run_chat: HashMap::new(),
            run_events: HashMap::new(),
            run_scroll: 0,
            events_scroll: 0,
            chat_input: String::new(),
            chat_input_cursor: 0,

            log_messages: VecDeque::with_capacity(1000),
            log_scroll: 0,

            show_new_task_dialog: false,
            new_task_agent: String::new(),
            new_task_input: String::new(),
            new_task_cursor: 0,
            new_task_field: 0,

            show_cancel_confirm: false,
            show_disconnect_confirm: false,
            show_quit_confirm: false,

            total_tasks: 0,
            completed_tasks: 0,
            failed_tasks: 0,
        }
    }

    pub fn uptime(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    pub fn add_log(&mut self, level: LogLevel, message: String) {
        self.log_messages.push_back(LogEntry {
            timestamp: Utc::now(),
            level,
            message,
        });
        while self.log_messages.len() > 1000 {
            self.log_messages.pop_front();
        }
    }

    pub fn worker_list(&self) -> Vec<&WorkerDisplayInfo> {
        let mut workers: Vec<_> = self.workers.values().collect();
        workers.sort_by(|a, b| a.worker_id.to_string().cmp(&b.worker_id.to_string()));
        workers
    }

    pub fn get_selected_worker(&self) -> Option<&WorkerDisplayInfo> {
        self.worker_list().get(self.selected_worker_index).copied()
    }

    pub fn task_display_list(&self) -> Vec<&TaskDisplayInfo> {
        self.task_list
            .iter()
            .filter_map(|id| self.tasks.get(id))
            .collect()
    }

    pub fn get_selected_task(&self) -> Option<&TaskDisplayInfo> {
        self.task_list
            .get(self.selected_task_index)
            .and_then(|id| self.tasks.get(id))
    }

    pub fn get_viewing_task(&self) -> Option<&TaskDisplayInfo> {
        self.viewing_task_id
            .as_ref()
            .and_then(|id| self.tasks.get(id))
    }
}

impl Default for ServerUiState {
    fn default() -> Self {
        Self::new()
    }
}
