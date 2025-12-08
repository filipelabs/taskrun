//! UI state for rendering.

use taskrun_proto::pb::{Task, Worker};

use crate::event::ConnectionState;

/// Available views in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Workers,
    WorkerDetail,
    Tasks,
    Runs,
    Trace,
}

/// Snapshot of data for rendering (no async, no locks).
pub struct UiState {
    /// List of workers from control plane.
    pub workers: Vec<Worker>,

    /// List of tasks from control plane.
    pub tasks: Vec<Task>,

    /// Current view/tab.
    pub current_view: View,

    /// Status message to display in footer.
    pub status_message: Option<String>,

    /// Current connection state.
    pub connection_state: ConnectionState,

    /// Last error message (if any).
    pub last_error: Option<String>,

    /// Number of consecutive poll failures (for UI feedback).
    pub consecutive_failures: u32,

    /// Selected worker index in the workers list.
    pub selected_worker_index: usize,

    /// Selected task index in the tasks list.
    pub selected_task_index: usize,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            workers: Vec::new(),
            tasks: Vec::new(),
            current_view: View::default(),
            status_message: Some("Connecting...".to_string()),
            connection_state: ConnectionState::Connecting,
            last_error: None,
            consecutive_failures: 0,
            selected_worker_index: 0,
            selected_task_index: 0,
        }
    }
}

impl UiState {
    /// Select the next worker in the list.
    pub fn select_next_worker(&mut self) {
        if !self.workers.is_empty() {
            self.selected_worker_index = (self.selected_worker_index + 1) % self.workers.len();
        }
    }

    /// Select the previous worker in the list.
    pub fn select_prev_worker(&mut self) {
        if !self.workers.is_empty() {
            self.selected_worker_index = self
                .selected_worker_index
                .checked_sub(1)
                .unwrap_or(self.workers.len().saturating_sub(1));
        }
    }

    /// Get the currently selected worker.
    pub fn selected_worker(&self) -> Option<&Worker> {
        self.workers.get(self.selected_worker_index)
    }

    /// Select the next task in the list.
    pub fn select_next_task(&mut self) {
        if !self.tasks.is_empty() {
            self.selected_task_index = (self.selected_task_index + 1) % self.tasks.len();
        }
    }

    /// Select the previous task in the list.
    pub fn select_prev_task(&mut self) {
        if !self.tasks.is_empty() {
            self.selected_task_index = self
                .selected_task_index
                .checked_sub(1)
                .unwrap_or(self.tasks.len().saturating_sub(1));
        }
    }

    /// Get the currently selected task.
    pub fn selected_task(&self) -> Option<&Task> {
        self.tasks.get(self.selected_task_index)
    }

    /// Check if the app should quit (used for Ctrl+C handling).
    pub fn should_quit(&self) -> bool {
        false // Could be set by signal handler
    }
}
