//! Event types for communication between background tasks and UI.

use std::time::Duration;

use ratatui::crossterm::event::KeyEvent;

use taskrun_proto::pb::{Task, Worker};

/// Connection state for the backend.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ConnectionState {
    /// Currently attempting to connect.
    #[default]
    Connecting,

    /// Successfully connected to the control plane.
    Connected,

    /// Disconnected, will retry after the specified duration.
    Disconnected { retry_in: Duration },
}

/// Events sent from background tasks to the UI thread.
#[allow(dead_code)] // Will be used in future issues
#[derive(Debug)]
pub enum UiEvent {
    /// Periodic tick for animations/refresh.
    Tick,

    /// Workers list was updated.
    WorkersUpdated(Vec<Worker>),

    /// Tasks list was updated.
    TasksUpdated(Vec<Task>),

    /// An error occurred.
    Error(String),

    /// Connection state changed.
    ConnectionStateChanged(ConnectionState),

    /// Key press from terminal.
    Key(KeyEvent),

    /// Request to quit the application.
    Quit,
}

/// Commands sent from UI to background tasks.
#[allow(dead_code)] // Will be used in future issues
#[derive(Debug)]
pub enum BackendCommand {
    /// Refresh workers list.
    RefreshWorkers,

    /// Refresh tasks list.
    RefreshTasks,

    /// Force immediate reconnect attempt.
    ForceReconnect,

    /// Select a task to view details.
    SelectTask(String),

    /// Cancel a task.
    CancelTask(String),

    /// Quit the application.
    Quit,
}
