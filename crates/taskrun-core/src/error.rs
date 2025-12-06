//! Core domain errors.

use thiserror::Error;

/// Core domain errors for TaskRun.
#[derive(Debug, Error)]
pub enum CoreError {
    /// Task not found.
    #[error("Task not found: {0}")]
    TaskNotFound(String),

    /// Run not found.
    #[error("Run not found: {0}")]
    RunNotFound(String),

    /// Worker not found.
    #[error("Worker not found: {0}")]
    WorkerNotFound(String),

    /// Invalid state transition.
    #[error("Invalid state transition: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },

    /// Agent not supported by worker.
    #[error("Agent '{agent}' not supported by worker '{worker}'")]
    AgentNotSupported { agent: String, worker: String },

    /// Invalid input.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),
}
