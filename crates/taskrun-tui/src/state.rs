//! UI state for rendering.

use taskrun_proto::pb::{Task, Worker};

use crate::event::ConnectionState;

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
        }
    }
}
