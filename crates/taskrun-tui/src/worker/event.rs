//! Worker TUI events and commands.

use std::time::Duration;

use crossterm::event::KeyEvent;

use super::state::{ConnectionState, LogLevel};

/// Events sent from the backend to the UI.
#[derive(Debug)]
#[allow(dead_code)] // Some variants are for API completeness
pub enum WorkerUiEvent {
    /// Periodic tick for UI refresh.
    Tick,
    /// Keyboard input.
    Key(KeyEvent),
    /// Connection state changed.
    ConnectionStateChanged(ConnectionState),
    /// A new run was assigned.
    RunStarted {
        run_id: String,
        task_id: String,
        agent: String,
    },
    /// Output from a run (streaming).
    RunProgress {
        run_id: String,
        output: String,
    },
    /// A run completed.
    RunCompleted {
        run_id: String,
        success: bool,
        error_message: Option<String>,
    },
    /// Log message from the worker.
    LogMessage {
        level: LogLevel,
        message: String,
    },
    /// Worker stats updated.
    StatsUpdated {
        active_runs: u32,
    },
    /// Request to quit.
    Quit,
}

/// Commands sent from the UI to the backend.
#[derive(Debug)]
pub enum WorkerCommand {
    /// Force reconnection to control plane.
    ForceReconnect,
    /// Quit the worker.
    Quit,
}

/// Helper to create a disconnect retry event.
pub fn disconnected_event(retry_in: Duration) -> WorkerUiEvent {
    WorkerUiEvent::ConnectionStateChanged(ConnectionState::Disconnected { retry_in })
}
