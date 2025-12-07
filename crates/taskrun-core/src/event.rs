//! Run execution events for tracking execution stages.

use crate::ids::{EventId, RunId, TaskId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A run execution event for tracking execution stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    /// Unique event identifier.
    pub id: EventId,
    /// Run this event belongs to.
    pub run_id: RunId,
    /// Task this event belongs to.
    pub task_id: TaskId,
    /// Type of event.
    pub event_type: RunEventType,
    /// Unix timestamp (milliseconds) when event occurred.
    pub timestamp_ms: i64,
    /// Event-specific metadata (tool_name, model, error, etc.).
    pub metadata: HashMap<String, String>,
}

impl RunEvent {
    /// Create a new run event.
    pub fn new(
        run_id: RunId,
        task_id: TaskId,
        event_type: RunEventType,
        metadata: HashMap<String, String>,
    ) -> Self {
        Self {
            id: EventId::generate(),
            run_id,
            task_id,
            event_type,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
            metadata,
        }
    }

    /// Create an ExecutionStarted event.
    pub fn execution_started(run_id: RunId, task_id: TaskId) -> Self {
        Self::new(
            run_id,
            task_id,
            RunEventType::ExecutionStarted,
            HashMap::new(),
        )
    }

    /// Create a SessionInitialized event with session and model info.
    pub fn session_initialized(
        run_id: RunId,
        task_id: TaskId,
        session_id: Option<String>,
        model: Option<String>,
    ) -> Self {
        let mut metadata = HashMap::new();
        if let Some(sid) = session_id {
            metadata.insert("session_id".to_string(), sid);
        }
        if let Some(m) = model {
            metadata.insert("model".to_string(), m);
        }
        Self::new(run_id, task_id, RunEventType::SessionInitialized, metadata)
    }

    /// Create a ToolRequested event.
    pub fn tool_requested(run_id: RunId, task_id: TaskId, tool_name: &str) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("tool_name".to_string(), tool_name.to_string());
        Self::new(run_id, task_id, RunEventType::ToolRequested, metadata)
    }

    /// Create a ToolCompleted event.
    pub fn tool_completed(run_id: RunId, task_id: TaskId, is_error: bool) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("is_error".to_string(), is_error.to_string());
        Self::new(run_id, task_id, RunEventType::ToolCompleted, metadata)
    }

    /// Create an OutputGenerated event.
    pub fn output_generated(run_id: RunId, task_id: TaskId, summary: Option<String>) -> Self {
        let mut metadata = HashMap::new();
        if let Some(s) = summary {
            metadata.insert("summary".to_string(), s);
        }
        Self::new(run_id, task_id, RunEventType::OutputGenerated, metadata)
    }

    /// Create an ExecutionCompleted event.
    pub fn execution_completed(run_id: RunId, task_id: TaskId, duration_ms: Option<i64>) -> Self {
        let mut metadata = HashMap::new();
        if let Some(d) = duration_ms {
            metadata.insert("duration_ms".to_string(), d.to_string());
        }
        Self::new(run_id, task_id, RunEventType::ExecutionCompleted, metadata)
    }

    /// Create an ExecutionFailed event.
    pub fn execution_failed(run_id: RunId, task_id: TaskId, error: Option<String>) -> Self {
        let mut metadata = HashMap::new();
        if let Some(e) = error {
            metadata.insert("error".to_string(), e);
        }
        Self::new(run_id, task_id, RunEventType::ExecutionFailed, metadata)
    }
}

/// Type of run execution event.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunEventType {
    /// Run execution begins.
    ExecutionStarted,
    /// Model/session initialized and captured.
    SessionInitialized,
    /// Tool call requested by the model.
    ToolRequested,
    /// Tool call finished.
    ToolCompleted,
    /// Text output generated (summary, not individual tokens).
    OutputGenerated,
    /// Execution completed successfully.
    ExecutionCompleted,
    /// Execution failed with error.
    ExecutionFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_started() {
        let run_id = RunId::generate();
        let task_id = TaskId::generate();
        let event = RunEvent::execution_started(run_id.clone(), task_id.clone());

        assert_eq!(event.run_id, run_id);
        assert_eq!(event.task_id, task_id);
        assert_eq!(event.event_type, RunEventType::ExecutionStarted);
        assert!(event.timestamp_ms > 0);
    }

    #[test]
    fn test_tool_requested() {
        let run_id = RunId::generate();
        let task_id = TaskId::generate();
        let event = RunEvent::tool_requested(run_id, task_id, "Read");

        assert_eq!(event.event_type, RunEventType::ToolRequested);
        assert_eq!(event.metadata.get("tool_name"), Some(&"Read".to_string()));
    }

    #[test]
    fn test_execution_failed() {
        let run_id = RunId::generate();
        let task_id = TaskId::generate();
        let event = RunEvent::execution_failed(run_id, task_id, Some("timeout".to_string()));

        assert_eq!(event.event_type, RunEventType::ExecutionFailed);
        assert_eq!(event.metadata.get("error"), Some(&"timeout".to_string()));
    }
}
