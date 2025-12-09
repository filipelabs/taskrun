//! Server TUI events and commands.

use taskrun_core::{RunId, RunStatus, TaskId, TaskStatus, WorkerId, WorkerStatus};

/// Events sent from backend to UI.
#[derive(Debug, Clone)]
pub enum ServerUiEvent {
    /// Server successfully started.
    ServerStarted {
        grpc_addr: String,
        http_addr: String,
    },

    /// Server failed to start.
    ServerError {
        message: String,
    },

    /// Worker connected.
    WorkerConnected {
        worker_id: WorkerId,
        hostname: String,
        agents: Vec<String>,
    },

    /// Worker disconnected.
    WorkerDisconnected {
        worker_id: WorkerId,
    },

    /// Worker heartbeat received.
    WorkerHeartbeat {
        worker_id: WorkerId,
        status: WorkerStatus,
        active_runs: u32,
    },

    /// Task created.
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
        status: RunStatus,
    },

    /// Run output chunk.
    RunOutputChunk {
        run_id: RunId,
        content: String,
    },

    /// Log message.
    LogMessage {
        level: LogLevel,
        message: String,
    },
}

/// Commands sent from UI to backend.
#[derive(Debug)]
pub enum ServerCommand {
    /// Create a new task.
    CreateTask {
        agent_name: String,
        input_json: String,
    },

    /// Cancel a task.
    CancelTask {
        task_id: TaskId,
    },

    /// Disconnect a worker.
    DisconnectWorker {
        worker_id: WorkerId,
    },

    /// Shutdown the server.
    Shutdown,
}

/// Log level for UI messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}
