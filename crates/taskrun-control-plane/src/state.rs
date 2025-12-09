//! Shared application state.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, mpsc, RwLock};

use taskrun_core::{RunEvent, RunEventType, RunId, RunStatus, Task, TaskId, TaskStatus, WorkerId, WorkerInfo, WorkerStatus};
use taskrun_proto::pb::RunServerMessage;

use crate::crypto::{BootstrapToken, CertificateAuthority};

// ============================================================================
// UI Notification Types
// ============================================================================

/// Notifications sent to the TUI for real-time updates.
#[derive(Debug, Clone)]
pub enum UiNotification {
    /// A worker connected to the control plane.
    WorkerConnected {
        worker_id: WorkerId,
        hostname: String,
        agents: Vec<String>,
    },
    /// A worker disconnected from the control plane.
    WorkerDisconnected {
        worker_id: WorkerId,
    },
    /// A worker sent a heartbeat.
    WorkerHeartbeat {
        worker_id: WorkerId,
        status: WorkerStatus,
        active_runs: u32,
        max_concurrent_runs: u32,
    },
    /// A new task was created.
    TaskCreated {
        task_id: TaskId,
        agent: String,
    },
    /// Task status changed.
    TaskStatusChanged {
        task_id: TaskId,
        status: TaskStatus,
    },
    /// Run status changed.
    RunStatusChanged {
        run_id: RunId,
        task_id: TaskId,
        worker_id: Option<WorkerId>,
        status: RunStatus,
    },
    /// Run output chunk received.
    RunOutputChunk {
        run_id: RunId,
        task_id: TaskId,
        content: String,
    },
    /// Run event occurred.
    RunEvent {
        run_id: RunId,
        task_id: TaskId,
        event_type: RunEventType,
    },
}

/// Type alias for UI notification sender.
pub type UiNotificationSender = broadcast::Sender<UiNotification>;

// ============================================================================
// Streaming Types
// ============================================================================

/// Events sent through the streaming channel for SSE subscribers.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum StreamEvent {
    /// Output chunk from worker.
    OutputChunk {
        seq: u64,
        content: String,
        is_final: bool,
        timestamp_ms: i64,
    },
    /// Status update (run started, completed, failed, cancelled).
    StatusUpdate {
        status: RunStatus,
        error_message: Option<String>,
        timestamp_ms: i64,
    },
}

/// Type alias for broadcast sender of stream events.
pub type StreamSender = broadcast::Sender<StreamEvent>;

// ============================================================================
// Connected Worker
// ============================================================================

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

    /// Broadcast channels for streaming run output, indexed by RunId.
    /// Created when a streaming client subscribes.
    pub stream_channels: RwLock<HashMap<RunId, StreamSender>>,

    /// Bootstrap tokens indexed by token hash.
    pub bootstrap_tokens: RwLock<HashMap<String, BootstrapToken>>,

    /// Certificate authority for signing worker CSRs.
    pub ca: Option<CertificateAuthority>,

    /// Optional channel for sending notifications to the TUI.
    pub ui_tx: Option<UiNotificationSender>,
}

impl AppState {
    /// Create a new AppState wrapped in Arc.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            workers: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            events: RwLock::new(HashMap::new()),
            outputs: RwLock::new(HashMap::new()),
            stream_channels: RwLock::new(HashMap::new()),
            bootstrap_tokens: RwLock::new(HashMap::new()),
            ca: None,
            ui_tx: None,
        })
    }

    /// Create a new AppState with a Certificate Authority.
    pub fn with_ca(ca: CertificateAuthority) -> Arc<Self> {
        Arc::new(Self {
            workers: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            events: RwLock::new(HashMap::new()),
            outputs: RwLock::new(HashMap::new()),
            stream_channels: RwLock::new(HashMap::new()),
            bootstrap_tokens: RwLock::new(HashMap::new()),
            ca: Some(ca),
            ui_tx: None,
        })
    }

    /// Create a new AppState with UI notification channel.
    /// Returns the AppState and a receiver for notifications.
    pub fn with_ui_channel(ca: Option<CertificateAuthority>) -> (Arc<Self>, broadcast::Receiver<UiNotification>) {
        let (tx, rx) = broadcast::channel(256);
        let state = Arc::new(Self {
            workers: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            events: RwLock::new(HashMap::new()),
            outputs: RwLock::new(HashMap::new()),
            stream_channels: RwLock::new(HashMap::new()),
            bootstrap_tokens: RwLock::new(HashMap::new()),
            ca,
            ui_tx: Some(tx),
        });
        (state, rx)
    }

    /// Send a notification to the UI if a channel is configured.
    pub fn notify_ui(&self, notification: UiNotification) {
        if let Some(ref tx) = self.ui_tx {
            // Ignore send errors (no subscribers = ok to drop)
            let _ = tx.send(notification);
        }
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
    #[allow(dead_code)]
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
        outputs.entry(run_id.clone()).or_default().push_str(content);
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

    // ========================================================================
    // Streaming Methods
    // ========================================================================

    /// Get or create a stream channel for a run.
    /// Returns the sender so callers can subscribe via `sender.subscribe()`.
    pub async fn get_or_create_stream_channel(&self, run_id: &RunId) -> StreamSender {
        let mut channels = self.stream_channels.write().await;
        channels
            .entry(run_id.clone())
            .or_insert_with(|| {
                // Capacity of 64 events should handle bursts
                let (tx, _) = broadcast::channel(64);
                tx
            })
            .clone()
    }

    /// Publish an event to a run's stream channel if it exists.
    pub async fn publish_stream_event(&self, run_id: &RunId, event: StreamEvent) {
        let channels = self.stream_channels.read().await;
        if let Some(tx) = channels.get(run_id) {
            // Ignore send errors (no subscribers = ok to drop)
            let _ = tx.send(event);
        }
    }

    /// Remove a stream channel (cleanup after run completes).
    pub async fn remove_stream_channel(&self, run_id: &RunId) {
        let mut channels = self.stream_channels.write().await;
        channels.remove(run_id);
    }

    // ========================================================================
    // Agent Validation
    // ========================================================================

    /// Check if any connected worker supports the given agent.
    pub async fn has_agent(&self, agent_name: &str) -> bool {
        let workers = self.workers.read().await;
        workers.values().any(|w| w.info.supports_agent(agent_name))
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            workers: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            events: RwLock::new(HashMap::new()),
            outputs: RwLock::new(HashMap::new()),
            stream_channels: RwLock::new(HashMap::new()),
            bootstrap_tokens: RwLock::new(HashMap::new()),
            ca: None,
            ui_tx: None,
        }
    }
}
