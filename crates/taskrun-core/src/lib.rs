//! TaskRun Core Domain Types
//!
//! This crate contains pure domain types with no dependencies on:
//! - Network/gRPC
//! - Database
//! - Runtime specifics
//!
//! All types here represent the core business domain of TaskRun.

pub mod error;
pub mod ids;
pub mod model;
pub mod status;
pub mod task;
pub mod worker;

// Re-export commonly used types
pub use error::CoreError;
pub use ids::{RunId, TaskId, WorkerId};
pub use model::{AgentSpec, ModelBackend};
pub use status::{RunStatus, TaskStatus, WorkerStatus};
pub use task::{RunSummary, Task};
pub use worker::WorkerInfo;
