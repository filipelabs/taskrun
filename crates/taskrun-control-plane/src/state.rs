//! Shared application state.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::{mpsc, RwLock};

use taskrun_core::{RunEvent, RunId, Task, TaskId, WorkerId, WorkerInfo, WorkerStatus};
use taskrun_proto::pb::RunServerMessage;

use crate::crypto::{BootstrapToken, CertificateAuthority};

/// Represents a connected worker's state.
#[allow(dead_code)]
pub struct ConnectedWorker {
    /// Worker information from WorkerHello.
    pub info: WorkerInfo,

    /// Current status from last heartbeat.
    pub status: WorkerStatus,

    /// Number of active runs.
    pub active_runs: u32,

    /// Maximum concurrent runs.
    pub max_concurrent_runs: u32,

    /// Timestamp of last heartbeat.
    pub last_heartbeat: DateTime<Utc>,

    /// Channel to send messages to this worker.
    pub tx: mpsc::Sender<RunServerMessage>,
}

/// Shared application state.
pub struct AppState {
    /// Connected workers indexed by WorkerId.
    pub workers: RwLock<HashMap<WorkerId, ConnectedWorker>>,

    /// Tasks indexed by TaskId.
    pub tasks: RwLock<HashMap<TaskId, Task>>,

    /// Run events indexed by RunId (each run can have multiple events).
    pub events: RwLock<HashMap<RunId, Vec<RunEvent>>>,

    /// Run output indexed by RunId (accumulated content from output chunks).
    pub outputs: RwLock<HashMap<RunId, String>>,

    /// Bootstrap tokens indexed by token hash.
    pub bootstrap_tokens: RwLock<HashMap<String, BootstrapToken>>,

    /// Certificate authority for signing worker CSRs.
    pub ca: Option<CertificateAuthority>,
}

impl AppState {
    /// Create a new AppState wrapped in Arc.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            workers: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            events: RwLock::new(HashMap::new()),
            outputs: RwLock::new(HashMap::new()),
            bootstrap_tokens: RwLock::new(HashMap::new()),
            ca: None,
        })
    }

    /// Create a new AppState with a Certificate Authority.
    pub fn with_ca(ca: CertificateAuthority) -> Arc<Self> {
        Arc::new(Self {
            workers: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            events: RwLock::new(HashMap::new()),
            outputs: RwLock::new(HashMap::new()),
            bootstrap_tokens: RwLock::new(HashMap::new()),
            ca: Some(ca),
        })
    }

    /// Get the number of connected workers.
    #[allow(dead_code)]
    pub async fn worker_count(&self) -> usize {
        self.workers.read().await.len()
    }

    /// Get the number of tasks.
    #[allow(dead_code)]
    pub async fn task_count(&self) -> usize {
        self.tasks.read().await.len()
    }

    /// Store a run event.
    pub async fn store_event(&self, event: RunEvent) {
        let run_id = event.run_id.clone();
        let mut events = self.events.write().await;
        events.entry(run_id).or_default().push(event);
    }

    /// Get all events for a run.
    pub async fn get_events_by_run(&self, run_id: &RunId) -> Vec<RunEvent> {
        let events = self.events.read().await;
        events.get(run_id).cloned().unwrap_or_default()
    }

    /// Get all events for a task (across all runs).
    pub async fn get_events_by_task(&self, task_id: &TaskId) -> Vec<RunEvent> {
        let events = self.events.read().await;
        let mut result = Vec::new();
        for run_events in events.values() {
            for event in run_events {
                if &event.task_id == task_id {
                    result.push(event.clone());
                }
            }
        }
        // Sort by timestamp
        result.sort_by_key(|e| e.timestamp_ms);
        result
    }

    /// Append output content to a run.
    pub async fn append_output(&self, run_id: &RunId, content: &str) {
        let mut outputs = self.outputs.write().await;
        outputs
            .entry(run_id.clone())
            .or_default()
            .push_str(content);
    }

    /// Get output for a run.
    pub async fn get_output(&self, run_id: &RunId) -> Option<String> {
        let outputs = self.outputs.read().await;
        outputs.get(run_id).cloned()
    }

    /// Get output for a task (finds the first run with output).
    pub async fn get_output_by_task(&self, task_id: &TaskId) -> Option<String> {
        // Get the task to find its runs
        let tasks = self.tasks.read().await;
        if let Some(task) = tasks.get(task_id) {
            let outputs = self.outputs.read().await;
            // Return output from the first run that has output
            for run in &task.runs {
                if let Some(output) = outputs.get(&run.run_id) {
                    return Some(output.clone());
                }
            }
        }
        None
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            workers: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            events: RwLock::new(HashMap::new()),
            outputs: RwLock::new(HashMap::new()),
            bootstrap_tokens: RwLock::new(HashMap::new()),
            ca: None,
        }
    }
}
