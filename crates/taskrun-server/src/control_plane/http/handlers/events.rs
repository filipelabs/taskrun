//! HTTP handlers for run events.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use taskrun_core::{RunEventType, TaskId};

use crate::control_plane::state::AppState;

/// Response structure for a run event.
#[derive(Serialize)]
pub struct EventResponse {
    pub id: String,
    pub run_id: String,
    pub task_id: String,
    pub event_type: String,
    pub timestamp_ms: i64,
    pub metadata: std::collections::HashMap<String, String>,
}

impl EventResponse {
    fn from_domain(event: &taskrun_core::RunEvent) -> Self {
        let event_type_str = match event.event_type {
            RunEventType::ExecutionStarted => "execution_started",
            RunEventType::SessionInitialized => "session_initialized",
            RunEventType::ToolRequested => "tool_requested",
            RunEventType::ToolCompleted => "tool_completed",
            RunEventType::OutputGenerated => "output_generated",
            RunEventType::ExecutionCompleted => "execution_completed",
            RunEventType::ExecutionFailed => "execution_failed",
        };

        Self {
            id: event.id.as_str().to_string(),
            run_id: event.run_id.as_str().to_string(),
            task_id: event.task_id.as_str().to_string(),
            event_type: event_type_str.to_string(),
            timestamp_ms: event.timestamp_ms,
            metadata: event.metadata.clone(),
        }
    }
}

/// Get events for a specific task.
///
/// GET /v1/tasks/:task_id/events
pub async fn get_task_events(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    let task_id = TaskId::new(&task_id);
    let events = state.get_events_by_task(&task_id).await;

    let response: Vec<EventResponse> = events.iter().map(EventResponse::from_domain).collect();

    (StatusCode::OK, Json(response))
}

/// Response structure for task output.
#[derive(Serialize)]
pub struct OutputResponse {
    pub task_id: String,
    pub output: Option<String>,
}

/// Get output for a specific task.
///
/// GET /v1/tasks/:task_id/output
pub async fn get_task_output(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    let task_id = TaskId::new(&task_id);
    let output = state.get_output_by_task(&task_id).await;

    let response = OutputResponse {
        task_id: task_id.as_str().to_string(),
        output,
    };

    (StatusCode::OK, Json(response))
}
