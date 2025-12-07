//! UI state for rendering.

use taskrun_proto::pb::{Task, Worker};

/// Available views in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Workers,
    Tasks,
    Runs,
    Trace,
}

/// Snapshot of data for rendering (no async, no locks).
#[derive(Default)]
pub struct UiState {
    /// List of workers from control plane.
    pub workers: Vec<Worker>,

    /// List of tasks from control plane.
    pub tasks: Vec<Task>,

    /// Current view/tab.
    pub current_view: View,

    /// Status message to display in footer.
    pub status_message: Option<String>,

    /// Whether we've successfully connected to the control plane.
    pub is_connected: bool,

    /// Last error message (if any).
    pub last_error: Option<String>,
}
