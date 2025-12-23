//! MCP request and response types.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ============================================================================
// Generic MCP Types
// ============================================================================

/// Generic MCP request wrapper.
#[derive(Debug, Deserialize)]
pub struct McpRequest<T> {
    /// Tool-specific parameters.
    pub params: T,
}

/// Generic MCP response wrapper.
#[derive(Debug, Serialize)]
pub struct McpResponse<T> {
    /// Tool result on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,

    /// Error message on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

impl<T> McpResponse<T> {
    /// Create a success response.
    pub fn ok(result: T) -> Self {
        Self {
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    pub fn err(code: &str, message: &str) -> Self {
        Self {
            result: None,
            error: Some(McpError {
                code: code.to_string(),
                message: message.to_string(),
            }),
        }
    }
}

/// MCP error details.
#[derive(Debug, Serialize)]
pub struct McpError {
    /// Error code.
    pub code: String,

    /// Human-readable error message.
    pub message: String,
}

// ============================================================================
// list_workers Types
// ============================================================================

/// Parameters for list_workers (none required).
#[derive(Debug, Deserialize, Default)]
pub struct ListWorkersParams {
    /// Optional filter by agent name.
    pub agent: Option<String>,
}

/// Result of list_workers.
#[derive(Debug, Serialize)]
pub struct ListWorkersResult {
    pub workers: Vec<WorkerInfo>,
}

/// Worker information.
#[derive(Debug, Serialize)]
pub struct WorkerInfo {
    pub worker_id: String,
    pub hostname: String,
    pub status: String,
    pub agents: Vec<String>,
    pub active_runs: u32,
    pub max_concurrent_runs: u32,
}

// ============================================================================
// start_new_task Types
// ============================================================================

/// Parameters for start_new_task.
#[derive(Debug, Deserialize)]
pub struct StartNewTaskParams {
    /// Agent to run the task.
    pub agent_name: String,

    /// Input for the agent (can be string or structured JSON).
    pub input: serde_json::Value,

    /// Optional metadata for the task.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Result of start_new_task.
#[derive(Debug, Serialize)]
pub struct StartNewTaskResult {
    pub task_id: String,
    pub run_id: String,
    pub status: String,
}

// ============================================================================
// read_task Types
// ============================================================================

/// Parameters for read_task.
#[derive(Debug, Deserialize)]
pub struct ReadTaskParams {
    /// Task ID to read.
    pub task_id: String,
}

/// Result of read_task.
#[derive(Debug, Serialize)]
pub struct ReadTaskResult {
    pub task_id: String,
    pub status: String,
    pub agent_name: String,
    pub created_at: String,

    /// Output from the task (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,

    /// Events from the task execution.
    pub events: Vec<TaskEvent>,

    /// Chat messages in the conversation.
    pub chat_messages: Vec<ChatMessageInfo>,

    /// Latest run's session ID (for continuation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Event information.
#[derive(Debug, Serialize)]
pub struct TaskEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp_ms: i64,

    /// Additional metadata from the event.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

/// Chat message information.
#[derive(Debug, Serialize)]
pub struct ChatMessageInfo {
    pub role: String,
    pub content: String,
    pub timestamp_ms: i64,
}

// ============================================================================
// continue_task Types
// ============================================================================

/// Parameters for continue_task.
#[derive(Debug, Deserialize)]
pub struct ContinueTaskParams {
    /// Task ID to continue.
    pub task_id: String,

    /// Follow-up message.
    pub message: String,
}

/// Result of continue_task.
#[derive(Debug, Serialize)]
pub struct ContinueTaskResult {
    pub task_id: String,
    pub run_id: String,
    pub status: String,
}
