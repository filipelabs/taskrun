//! JSON output for streaming events to stdout.

use serde::Serialize;
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag to enable JSON output mode.
static JSON_MODE_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable JSON output mode.
pub fn enable_json_mode() {
    JSON_MODE_ENABLED.store(true, Ordering::SeqCst);
}

/// Check if JSON mode is enabled.
pub fn is_json_mode() -> bool {
    JSON_MODE_ENABLED.load(Ordering::SeqCst)
}

/// JSON event types that can be emitted.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonEventType {
    WorkerConnected,
    WorkerDisconnected,
    TaskAssigned,
    TaskRunning,
    OutputChunk,
    TaskCompleted,
    TaskFailed,
    TaskCancelled,
    Heartbeat,
    ContinueReceived,
    Error,
}

/// A JSON event to be output to stdout.
#[derive(Debug, Clone, Serialize)]
pub struct JsonEvent {
    pub event: JsonEventType,
    pub timestamp: String,
    pub data: serde_json::Value,
}

impl JsonEvent {
    /// Create a new JSON event with the current timestamp.
    pub fn new(event: JsonEventType, data: serde_json::Value) -> Self {
        Self {
            event,
            timestamp: chrono::Utc::now().to_rfc3339(),
            data,
        }
    }

    /// Output this event as a JSON line to stdout.
    pub fn emit(&self) {
        if !is_json_mode() {
            return;
        }
        if let Ok(json) = serde_json::to_string(self) {
            let mut stdout = io::stdout().lock();
            let _ = writeln!(stdout, "{}", json);
            let _ = stdout.flush();
        }
    }
}

/// Emit a worker_connected event.
pub fn emit_worker_connected(worker_id: &str, endpoint: &str) {
    JsonEvent::new(
        JsonEventType::WorkerConnected,
        serde_json::json!({
            "worker_id": worker_id,
            "endpoint": endpoint,
        }),
    )
    .emit();
}

/// Emit a worker_disconnected event.
pub fn emit_worker_disconnected(worker_id: &str, reason: Option<&str>) {
    JsonEvent::new(
        JsonEventType::WorkerDisconnected,
        serde_json::json!({
            "worker_id": worker_id,
            "reason": reason,
        }),
    )
    .emit();
}

/// Emit a task_assigned event.
pub fn emit_task_assigned(run_id: &str, task_id: &str, agent_name: &str) {
    JsonEvent::new(
        JsonEventType::TaskAssigned,
        serde_json::json!({
            "run_id": run_id,
            "task_id": task_id,
            "agent_name": agent_name,
        }),
    )
    .emit();
}

/// Emit a task_running event.
pub fn emit_task_running(run_id: &str) {
    JsonEvent::new(
        JsonEventType::TaskRunning,
        serde_json::json!({
            "run_id": run_id,
        }),
    )
    .emit();
}

/// Emit an output_chunk event.
pub fn emit_output_chunk(run_id: &str, seq: u64, content: &str, is_final: bool) {
    JsonEvent::new(
        JsonEventType::OutputChunk,
        serde_json::json!({
            "run_id": run_id,
            "seq": seq,
            "content": content,
            "is_final": is_final,
        }),
    )
    .emit();
}

/// Emit a task_completed event.
pub fn emit_task_completed(run_id: &str, model_used: Option<&str>, provider: Option<&str>) {
    JsonEvent::new(
        JsonEventType::TaskCompleted,
        serde_json::json!({
            "run_id": run_id,
            "model_used": model_used,
            "provider": provider,
        }),
    )
    .emit();
}

/// Emit a task_failed event.
pub fn emit_task_failed(run_id: &str, error: &str) {
    JsonEvent::new(
        JsonEventType::TaskFailed,
        serde_json::json!({
            "run_id": run_id,
            "error": error,
        }),
    )
    .emit();
}

/// Emit a task_cancelled event.
pub fn emit_task_cancelled(run_id: &str, reason: &str) {
    JsonEvent::new(
        JsonEventType::TaskCancelled,
        serde_json::json!({
            "run_id": run_id,
            "reason": reason,
        }),
    )
    .emit();
}

/// Emit a heartbeat event.
pub fn emit_heartbeat(worker_id: &str, status: &str, active_runs: u32) {
    JsonEvent::new(
        JsonEventType::Heartbeat,
        serde_json::json!({
            "worker_id": worker_id,
            "status": status,
            "active_runs": active_runs,
        }),
    )
    .emit();
}

/// Emit a continue_received event.
pub fn emit_continue_received(run_id: &str, message_len: usize) {
    JsonEvent::new(
        JsonEventType::ContinueReceived,
        serde_json::json!({
            "run_id": run_id,
            "message_len": message_len,
        }),
    )
    .emit();
}

/// Emit an error event.
pub fn emit_error(message: &str, details: Option<HashMap<String, String>>) {
    JsonEvent::new(
        JsonEventType::Error,
        serde_json::json!({
            "message": message,
            "details": details,
        }),
    )
    .emit();
}
