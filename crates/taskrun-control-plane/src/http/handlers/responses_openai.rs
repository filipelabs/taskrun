//! HTTP handler for OpenAI-compatible `/v1/responses` endpoint.
//!
//! This module implements the OpenAI Responses API, allowing clients to use
//! familiar OpenAI SDKs while TaskRun orchestrates agents on workers.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};

use taskrun_core::{Task, TaskStatus};

use crate::scheduler::Scheduler;
use crate::state::AppState;

// ============================================================================
// Request Types
// ============================================================================

/// OpenAI-style request body for POST /v1/responses.
#[derive(Debug, Deserialize)]
pub struct CreateResponseRequest {
    /// Model identifier (maps to agent_name).
    pub model: String,

    /// Input - can be a simple string or structured content.
    pub input: Value,

    /// Optional system instructions.
    #[serde(default)]
    pub instructions: Option<String>,

    /// Whether to stream the response (not yet implemented).
    #[serde(default)]
    pub stream: bool,

    /// Maximum output tokens.
    #[serde(default)]
    pub max_output_tokens: Option<u32>,

    /// Temperature for sampling.
    #[serde(default)]
    pub temperature: Option<f32>,

    /// Arbitrary metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

// ============================================================================
// Response Types
// ============================================================================

/// OpenAI-style response object.
#[derive(Debug, Serialize)]
pub struct ResponseObject {
    /// Response ID (derived from run_id).
    pub id: String,

    /// Object type - always "response".
    pub object: &'static str,

    /// Unix timestamp when created.
    pub created_at: i64,

    /// Response status: "in_progress", "completed", "failed", "cancelled".
    pub status: String,

    /// Model that was used.
    pub model: String,

    /// Output content blocks.
    pub output: Vec<OutputItem>,

    /// Token usage (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,

    /// Error details (if failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObject>,

    /// Metadata including internal IDs.
    pub metadata: HashMap<String, String>,
}

/// Output item in the response.
#[derive(Debug, Serialize)]
pub struct OutputItem {
    /// Item type - "message" for assistant messages.
    #[serde(rename = "type")]
    pub item_type: String,

    /// Unique item ID.
    pub id: String,

    /// Role - "assistant" for output messages.
    pub role: String,

    /// Item status.
    pub status: String,

    /// Content blocks.
    pub content: Vec<ContentBlock>,
}

/// Content block within an output item.
#[derive(Debug, Serialize)]
pub struct ContentBlock {
    /// Block type - "output_text" for text content.
    #[serde(rename = "type")]
    pub block_type: String,

    /// MIME type of the content.
    pub content_type: String,

    /// Text content.
    pub text: String,
}

/// Token usage statistics.
#[derive(Debug, Serialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

/// Error details for failed responses.
#[derive(Debug, Serialize)]
pub struct ErrorObject {
    pub message: String,

    #[serde(rename = "type")]
    pub error_type: String,

    pub code: String,
}

/// OpenAI-style error response wrapper.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorObject,
}

// ============================================================================
// Handler
// ============================================================================

/// POST /v1/responses - Create a response (OpenAI-compatible).
pub async fn create_response(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateResponseRequest>,
) -> impl IntoResponse {
    info!(
        model = %req.model,
        stream = req.stream,
        "Received OpenAI-compatible request"
    );

    // TODO: Implement streaming (Phase 3)
    if req.stream {
        return (
            StatusCode::NOT_IMPLEMENTED,
            Json(ErrorResponse {
                error: ErrorObject {
                    message: "Streaming is not yet implemented".to_string(),
                    error_type: "invalid_request_error".to_string(),
                    code: "streaming_not_supported".to_string(),
                },
            }),
        )
            .into_response();
    }

    // Map model to agent_name (direct mapping for MVP)
    let agent_name = resolve_agent_name(&req.model);

    // Build input_json from request
    let input_json = build_input_json(&req);

    // Create task
    let mut task = Task::new(&agent_name, &input_json, "http-api");
    for (k, v) in &req.metadata {
        task.labels.insert(k.clone(), v.clone());
    }
    task.labels.insert("source".to_string(), "openai_api".to_string());

    let task_id = task.id.clone();

    info!(
        task_id = %task_id,
        agent = %agent_name,
        "Creating task from OpenAI request"
    );

    // Store task
    state.tasks.write().await.insert(task_id.clone(), task);

    // Schedule task
    let scheduler = Scheduler::new(state.clone());
    let run_id = match scheduler.assign_task(&task_id).await {
        Ok(run_id) => {
            info!(task_id = %task_id, run_id = %run_id, "Task assigned to worker");
            Some(run_id)
        }
        Err(e) => {
            warn!(task_id = %task_id, error = %e, "Failed to assign task");
            None
        }
    };

    // Wait for task completion (poll with timeout)
    let timeout = Duration::from_secs(300); // 5 minute timeout
    let poll_interval = Duration::from_millis(100);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            warn!(task_id = %task_id, "Task timed out");
            return (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: ErrorObject {
                        message: "Task execution timed out".to_string(),
                        error_type: "timeout_error".to_string(),
                        code: "task_timeout".to_string(),
                    },
                }),
            )
                .into_response();
        }

        // Check task status
        let task_opt = state.tasks.read().await.get(&task_id).cloned();
        let task = match task_opt {
            Some(t) => t,
            None => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: ErrorObject {
                            message: "Task disappeared".to_string(),
                            error_type: "internal_error".to_string(),
                            code: "task_not_found".to_string(),
                        },
                    }),
                )
                    .into_response();
            }
        };

        if task.is_terminal() {
            // Task completed, build response
            let response = build_response(&state, &task, &req.model, run_id.as_ref()).await;
            return (StatusCode::OK, Json(response)).into_response();
        }

        // Still running, wait and poll again
        tokio::time::sleep(poll_interval).await;
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Resolve model name to agent name.
/// For MVP: direct mapping (strip "taskrun:" prefix if present).
fn resolve_agent_name(model: &str) -> String {
    if let Some(agent) = model.strip_prefix("taskrun:") {
        agent.to_string()
    } else {
        model.to_string()
    }
}

/// Build input_json from the OpenAI request.
fn build_input_json(req: &CreateResponseRequest) -> String {
    // Extract the actual input text
    let input_text = match &req.input {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            // Handle structured input (messages array)
            // For MVP, just extract text from the last user message
            arr.iter()
                .filter_map(|item| {
                    item.get("content")
                        .and_then(|c| c.as_str())
                        .map(String::from)
                })
                .last()
                .unwrap_or_default()
        }
        _ => serde_json::to_string(&req.input).unwrap_or_default(),
    };

    // Build the input object for the agent
    let mut input = serde_json::json!({
        "task": input_text,
    });

    // Add optional fields
    if let Some(instructions) = &req.instructions {
        input["instructions"] = Value::String(instructions.clone());
    }
    if let Some(max_tokens) = req.max_output_tokens {
        input["max_output_tokens"] = Value::Number(max_tokens.into());
    }
    if let Some(temp) = req.temperature {
        input["temperature"] = Value::Number(serde_json::Number::from_f64(temp as f64).unwrap());
    }
    if !req.metadata.is_empty() {
        input["metadata"] = serde_json::to_value(&req.metadata).unwrap();
    }

    serde_json::to_string(&input).unwrap_or_default()
}

/// Build the OpenAI-style response from task state.
async fn build_response(
    state: &AppState,
    task: &Task,
    model: &str,
    run_id: Option<&taskrun_core::RunId>,
) -> ResponseObject {
    // Get output from the run
    let output_text = if let Some(rid) = run_id {
        state.get_output(rid).await.unwrap_or_default()
    } else if let Some(run) = task.latest_run() {
        state.get_output(&run.run_id).await.unwrap_or_default()
    } else {
        String::new()
    };

    // Map task status to response status
    let status = match task.status {
        TaskStatus::Pending => "in_progress",
        TaskStatus::Running => "in_progress",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
    };

    // Build error if failed
    let error = if task.status == TaskStatus::Failed {
        let error_msg = task
            .latest_run()
            .and_then(|r| r.error_message.clone())
            .unwrap_or_else(|| "Unknown error".to_string());

        Some(ErrorObject {
            message: error_msg,
            error_type: "agent_error".to_string(),
            code: "execution_failed".to_string(),
        })
    } else {
        None
    };

    // Build output blocks
    let output = if !output_text.is_empty() && task.status == TaskStatus::Completed {
        vec![OutputItem {
            item_type: "message".to_string(),
            id: format!("msg_{}", task.id.as_str()),
            role: "assistant".to_string(),
            status: "completed".to_string(),
            content: vec![ContentBlock {
                block_type: "output_text".to_string(),
                content_type: "text/plain".to_string(),
                text: output_text,
            }],
        }]
    } else {
        vec![]
    };

    // Build metadata
    let mut metadata = HashMap::new();
    metadata.insert("task_id".to_string(), task.id.as_str().to_string());
    if let Some(run) = task.latest_run() {
        metadata.insert("run_id".to_string(), run.run_id.as_str().to_string());
        metadata.insert("worker_id".to_string(), run.worker_id.as_str().to_string());
    }

    // Derive response ID from run_id or task_id
    let response_id = if let Some(run) = task.latest_run() {
        format!("resp_{}", run.run_id.as_str())
    } else {
        format!("resp_{}", task.id.as_str())
    };

    ResponseObject {
        id: response_id,
        object: "response",
        created_at: task.created_at.timestamp(),
        status: status.to_string(),
        model: model.to_string(),
        output,
        usage: None, // TODO: Track token usage
        error,
        metadata,
    }
}
