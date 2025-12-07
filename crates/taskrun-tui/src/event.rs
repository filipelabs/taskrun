//! Event types for communication between background tasks and UI.

use ratatui::crossterm::event::KeyEvent;

use taskrun_proto::pb::{Task, Worker};

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

    /// Select a task to view details.
    SelectTask(String),

    /// Cancel a task.
    CancelTask(String),

    /// Quit the application.
    Quit,
}
