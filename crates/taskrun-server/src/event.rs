//! Server TUI events and commands.

use taskrun_core::{ChatRole, RunId, RunStatus, TaskId, TaskStatus, WorkerId, WorkerStatus};
use taskrun_proto::pb::RunServerMessage;

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
        max_concurrent_runs: u32,
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

    /// Chat message (user or assistant message in conversation).
    ChatMessage {
        run_id: RunId,
        task_id: TaskId,
        role: ChatRole,
        content: String,
    },

    /// Log message.
    LogMessage {
        level: LogLevel,
        message: String,
    },
}

/// Commands sent from UI to backend.
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

    /// Send a chat message to a run (forwarded to worker).
    SendChatMessage {
        run_id: RunId,
        message: String,
    },

    /// Shutdown the server.
    Shutdown,
}

/// A message to send to a specific worker.
pub struct WorkerMessage {
    pub worker_id: WorkerId,
    pub message: RunServerMessage,
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
