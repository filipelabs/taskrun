//! Shared application state.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::{mpsc, RwLock};

use taskrun_core::{WorkerId, WorkerInfo, WorkerStatus};
use taskrun_proto::pb::RunServerMessage;

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
}

impl AppState {
    /// Create a new AppState wrapped in Arc.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            workers: RwLock::new(HashMap::new()),
        })
    }

    /// Get the number of connected workers.
    #[allow(dead_code)]
    pub async fn worker_count(&self) -> usize {
        self.workers.read().await.len()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            workers: RwLock::new(HashMap::new()),
        }
    }
}
