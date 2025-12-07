//! Shared application state.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::{mpsc, RwLock};

use taskrun_core::{Task, TaskId, WorkerId, WorkerInfo, WorkerStatus};
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
            bootstrap_tokens: RwLock::new(HashMap::new()),
            ca: None,
        })
    }

    /// Create a new AppState with a Certificate Authority.
    pub fn with_ca(ca: CertificateAuthority) -> Arc<Self> {
        Arc::new(Self {
            workers: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
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
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            workers: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            bootstrap_tokens: RwLock::new(HashMap::new()),
            ca: None,
        }
    }
}
