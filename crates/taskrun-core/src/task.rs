//! Task and Run types.

use crate::{ModelBackend, RunId, RunStatus, TaskId, TaskStatus, WorkerId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A Task represents a logical unit of work in the control plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier.
    pub id: TaskId,

    /// Name of the agent to execute.
    pub agent_name: String,

    /// Input payload as JSON string.
    pub input_json: String,

    /// Current task status.
    pub status: TaskStatus,

    /// Who created this task.
    pub created_by: String,

    /// When the task was created.
    pub created_at: DateTime<Utc>,

    /// Task labels/metadata.
    pub labels: HashMap<String, String>,

    /// Runs associated with this task.
    pub runs: Vec<RunSummary>,
}

impl Task {
    /// Create a new Task.
    pub fn new(
        agent_name: impl Into<String>,
        input_json: impl Into<String>,
        created_by: impl Into<String>,
    ) -> Self {
        Self {
            id: TaskId::generate(),
            agent_name: agent_name.into(),
            input_json: input_json.into(),
            status: TaskStatus::Pending,
            created_by: created_by.into(),
            created_at: Utc::now(),
            labels: HashMap::new(),
            runs: Vec::new(),
        }
    }

    /// Builder method to add a label.
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Builder method to set a specific ID (useful for testing).
    pub fn with_id(mut self, id: TaskId) -> Self {
        self.id = id;
        self
    }

    /// Add a run to this task.
    pub fn add_run(&mut self, run: RunSummary) {
        self.runs.push(run);
    }

    /// Get the most recent run, if any.
    pub fn latest_run(&self) -> Option<&RunSummary> {
        self.runs.last()
    }

    /// Check if the task is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }
}

/// Summary of a Run associated with a Task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSummary {
    /// Unique run identifier.
    pub run_id: RunId,

    /// Worker that executed/is executing this run.
    pub worker_id: WorkerId,

    /// Current run status.
    pub status: RunStatus,

    /// When the run started (assigned to worker).
    pub started_at: Option<DateTime<Utc>>,

    /// When the run finished (if terminal).
    pub finished_at: Option<DateTime<Utc>>,

    /// Model backend actually used for this run.
    pub backend_used: Option<ModelBackend>,

    /// Error message if run failed.
    pub error_message: Option<String>,
}

impl RunSummary {
    /// Create a new RunSummary.
    pub fn new(worker_id: WorkerId) -> Self {
        Self {
            run_id: RunId::generate(),
            worker_id,
            status: RunStatus::Pending,
            started_at: None,
            finished_at: None,
            backend_used: None,
            error_message: None,
        }
    }

    /// Mark the run as started.
    pub fn start(&mut self) {
        self.status = RunStatus::Running;
        self.started_at = Some(Utc::now());
    }

    /// Mark the run as completed.
    pub fn complete(&mut self, backend: Option<ModelBackend>) {
        self.status = RunStatus::Completed;
        self.finished_at = Some(Utc::now());
        self.backend_used = backend;
    }

    /// Mark the run as failed.
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = RunStatus::Failed;
        self.finished_at = Some(Utc::now());
        self.error_message = Some(error.into());
    }

    /// Mark the run as cancelled.
    pub fn cancel(&mut self) {
        self.status = RunStatus::Cancelled;
        self.finished_at = Some(Utc::now());
    }
}
