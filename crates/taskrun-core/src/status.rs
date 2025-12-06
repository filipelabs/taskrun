//! Status enums for Tasks, Runs, and Workers.

use serde::{Deserialize, Serialize};

/// Status of a Task in the control plane.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    /// Task created but not yet assigned to a worker.
    #[default]
    Pending,
    /// Task has at least one active Run.
    Running,
    /// Task completed successfully.
    Completed,
    /// Task failed (all Runs failed).
    Failed,
    /// Task was cancelled by user or system.
    Cancelled,
}

/// Status of a Run on a specific worker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunStatus {
    /// Run created but not yet sent to worker.
    #[default]
    Pending,
    /// Run sent to worker, awaiting acknowledgment.
    Assigned,
    /// Run actively executing on worker.
    Running,
    /// Run completed successfully.
    Completed,
    /// Run failed.
    Failed,
    /// Run was cancelled.
    Cancelled,
}

impl RunStatus {
    /// Returns true if the run is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Returns true if the run is still active (not terminal).
    pub fn is_active(&self) -> bool {
        !self.is_terminal()
    }
}

/// Status of a Worker connection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkerStatus {
    /// Worker is idle and ready to accept runs.
    #[default]
    Idle,
    /// Worker is busy processing runs.
    Busy,
    /// Worker is draining (not accepting new runs).
    Draining,
    /// Worker is in an error state.
    Error,
}

impl WorkerStatus {
    /// Returns true if the worker can accept new runs.
    pub fn can_accept_runs(&self) -> bool {
        matches!(self, Self::Idle | Self::Busy)
    }
}
