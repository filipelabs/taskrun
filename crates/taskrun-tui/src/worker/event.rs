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
        input: String,
    },
    /// Output from a run (streaming).
    RunProgress { run_id: String, output: String },
    /// A run completed.
    RunCompleted {
        run_id: String,
        success: bool,
        error_message: Option<String>,
    },
    /// An event occurred during a run (tool use, execution lifecycle, etc).
    RunEvent {
        run_id: String,
        event_type: String,
        details: Option<String>,
    },
    /// Log message from the worker.
    LogMessage { level: LogLevel, message: String },
    /// Worker stats updated.
    StatsUpdated { active_runs: u32 },
    /// Session ID captured for a run (enables continuation).
    SessionCaptured { run_id: String, session_id: String },
    /// A continuation turn completed (finalize output as assistant message).
    TurnCompleted { run_id: String },
    /// A user message was added to a run (from server or local input).
    UserMessageAdded { run_id: String, message: String },
    /// Request to quit.
    Quit,
}

/// Commands sent from the UI to the backend.
#[derive(Debug)]
pub enum WorkerCommand {
    /// Force reconnection to control plane.
    ForceReconnect,
    /// Continue a run with a follow-up message.
    ContinueRun {
        run_id: String,
        session_id: String,
        message: String,
    },
    /// Create a new task.
    CreateTask { prompt: String },
    /// Quit the worker.
    Quit,
}

/// Helper to create a disconnect retry event.
pub fn disconnected_event(retry_in: Duration) -> WorkerUiEvent {
    WorkerUiEvent::ConnectionStateChanged(ConnectionState::Disconnected { retry_in })
}
