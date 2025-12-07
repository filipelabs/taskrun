//! HTTP handler for OpenAI-compatible `/v1/responses` endpoint.
//!
//! This module implements the OpenAI Responses API, allowing clients to use
//! familiar OpenAI SDKs while TaskRun orchestrates agents on workers.

use std::collections::HashMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{rejection::JsonRejection, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use futures_util::stream::{self, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;
use tracing::{info, warn};

use taskrun_core::{RunStatus, Task, TaskStatus};

use crate::scheduler::Scheduler;
use crate::state::{AppState, StreamEvent};

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
#[derive(Debug, Clone, Serialize)]
pub struct ErrorObject {
    pub message: String,

    #[serde(rename = "type")]
    pub error_type: String,

    pub code: String,

    /// Optional parameter name for validation errors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
}

/// OpenAI-style error response wrapper.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorObject,
}

// ============================================================================
// API Error Type
// ============================================================================

/// API errors with proper HTTP status codes and OpenAI-style error responses.
#[derive(Debug)]
pub enum ApiError {
    // Client errors (4xx)
    /// Invalid JSON in request body.
    InvalidJson { message: String },
    /// Missing required field.
    MissingField { field: &'static str },
    /// Invalid field value.
    InvalidField {
        field: &'static str,
        message: String,
    },
    /// Model/agent not found.
    ModelNotFound { model: String },

    // Server errors (5xx)
    /// No workers available for the requested agent.
    NoWorkersAvailable { agent: String },
    /// Worker disconnected during execution.
    WorkerDisconnected,
    /// Agent execution failed.
    ExecutionFailed { message: String },
    /// Task execution timed out.
    TaskTimeout,
    /// Internal server error.
    Internal { message: String },
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_type, code, message, param) = match self {
            ApiError::InvalidJson { message } => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "invalid_json",
                message,
                None,
            ),
            ApiError::MissingField { field } => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "missing_field",
                format!("Missing required field: {}", field),
                Some(field.to_string()),
            ),
            ApiError::InvalidField { field, message } => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "invalid_field",
                format!("Invalid field '{}': {}", field, message),
                Some(field.to_string()),
            ),
            ApiError::ModelNotFound { model } => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "model_not_found",
                format!("Model '{}' not found", model),
                Some("model".to_string()),
            ),
            ApiError::NoWorkersAvailable { agent } => (
                StatusCode::SERVICE_UNAVAILABLE,
                "server_error",
                "no_workers_available",
                format!("No workers available for agent '{}'", agent),
                None,
            ),
            ApiError::WorkerDisconnected => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "worker_disconnected",
                "Worker disconnected during execution".to_string(),
                None,
            ),
            ApiError::ExecutionFailed { message } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "agent_error",
                "execution_failed",
                message,
                None,
            ),
            ApiError::TaskTimeout => (
                StatusCode::GATEWAY_TIMEOUT,
                "timeout_error",
                "task_timeout",
                "Task execution timed out".to_string(),
                None,
            ),
            ApiError::Internal { message } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal_error",
                message,
                None,
            ),
        };

        let body = ErrorResponse {
            error: ErrorObject {
                message,
                error_type: error_type.to_string(),
                code: code.to_string(),
                param,
            },
        };

        (status, Json(body)).into_response()
    }
}

// ============================================================================
// SSE Event Types
// ============================================================================

/// Type alias for boxed SSE stream.
type SseEventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

/// SSE event: response.created
#[derive(Debug, Serialize)]
struct ResponseCreatedEvent {
    id: String,
    object: &'static str,
    model: String,
    created_at: i64,
}

/// SSE event: response.output_text.delta
#[derive(Debug, Serialize)]
struct OutputTextDeltaEvent {
    response_id: String,
    output_index: u32,
    delta: DeltaContent,
}

/// Delta content for streaming.
#[derive(Debug, Serialize)]
struct DeltaContent {
    content_type: String,
    text: String,
}

/// SSE event: response.completed
#[derive(Debug, Serialize)]
struct ResponseCompletedEvent {
    id: String,
    status: String,
    output: Vec<OutputItem>,
    usage: Option<Usage>,
}

/// SSE event: response.failed
#[derive(Debug, Serialize)]
struct ResponseFailedEvent {
    id: String,
    status: String,
    error: ErrorObject,
}

// ============================================================================
// Handler
// ============================================================================

/// POST /v1/responses - Create a response (OpenAI-compatible).
pub async fn create_response(
    State(state): State<Arc<AppState>>,
    json_result: Result<Json<CreateResponseRequest>, JsonRejection>,
) -> Response {
    // Handle JSON parsing errors
    let req = match json_result {
        Ok(Json(req)) => req,
        Err(rejection) => {
            warn!(error = %rejection, "Invalid JSON in request body");
            return ApiError::InvalidJson {
                message: rejection.body_text(),
            }
            .into_response();
        }
    };

    info!(
        model = %req.model,
        stream = req.stream,
        "Received OpenAI-compatible request"
    );

    // Validate request
    if let Err(e) = validate_request(&req) {
        return e.into_response();
    }

    // Validate model/agent exists
    let agent_name = resolve_agent_name(&req.model);
    if !state.has_agent(&agent_name).await {
        warn!(model = %req.model, agent = %agent_name, "Model not found");
        return ApiError::ModelNotFound {
            model: req.model.clone(),
        }
        .into_response();
    }

    if req.stream {
        create_streaming_response(state, req).await.into_response()
    } else {
        create_non_streaming_response(state, req)
            .await
            .into_response()
    }
}

/// Validate request fields before processing.
fn validate_request(req: &CreateResponseRequest) -> Result<(), ApiError> {
    // Model is required and non-empty
    if req.model.trim().is_empty() {
        return Err(ApiError::MissingField { field: "model" });
    }

    // Input is required and non-null
    if req.input.is_null() {
        return Err(ApiError::MissingField { field: "input" });
    }

    // max_output_tokens must be positive if present
    if let Some(tokens) = req.max_output_tokens {
        if tokens == 0 {
            return Err(ApiError::InvalidField {
                field: "max_output_tokens",
                message: "must be greater than 0".to_string(),
            });
        }
    }

    // temperature must be 0.0-2.0 if present
    if let Some(temp) = req.temperature {
        if !(0.0..=2.0).contains(&temp) {
            return Err(ApiError::InvalidField {
                field: "temperature",
                message: "must be between 0.0 and 2.0".to_string(),
            });
        }
    }

    Ok(())
}

/// Create a streaming SSE response.
async fn create_streaming_response(
    state: Arc<AppState>,
    req: CreateResponseRequest,
) -> Sse<SseEventStream> {
    // Map model to agent_name
    let agent_name = resolve_agent_name(&req.model);
    let input_json = build_input_json(&req);

    // Create task
    let mut task = Task::new(&agent_name, &input_json, "http-api");
    for (k, v) in &req.metadata {
        task.labels.insert(k.clone(), v.clone());
    }
    task.labels
        .insert("source".to_string(), "openai_api".to_string());
    task.labels
        .insert("streaming".to_string(), "true".to_string());

    let task_id = task.id.clone();
    let created_at = task.created_at.timestamp();

    info!(
        task_id = %task_id,
        agent = %agent_name,
        "Creating streaming task from OpenAI request"
    );

    // Store task
    state.tasks.write().await.insert(task_id.clone(), task);

    // Schedule task
    let scheduler = Scheduler::new(state.clone());
    let run_id = match scheduler.assign_task(&task_id).await {
        Ok(run_id) => {
            info!(task_id = %task_id, run_id = %run_id, "Task assigned to worker (streaming)");
            run_id
        }
        Err(e) => {
            warn!(task_id = %task_id, error = %e, "Failed to assign task");
            // Return error as SSE stream with single error event
            let error_stream: SseEventStream = Box::pin(stream::once(async move {
                let event = ResponseFailedEvent {
                    id: format!("resp_{}", task_id.as_str()),
                    status: "failed".to_string(),
                    error: ErrorObject {
                        message: format!("Failed to schedule task: {}", e),
                        error_type: "server_error".to_string(),
                        code: "no_workers_available".to_string(),
                        param: None,
                    },
                };
                Ok::<_, Infallible>(
                    Event::default()
                        .event("response.failed")
                        .json_data(event)
                        .unwrap(),
                )
            }));
            return Sse::new(error_stream).keep_alive(KeepAlive::default());
        }
    };

    let response_id = format!("resp_{}", run_id.as_str());

    // Subscribe to stream channel BEFORE any events might be published
    let sender = state.get_or_create_stream_channel(&run_id).await;
    let receiver = sender.subscribe();

    // Create the SSE stream
    let sse_stream: SseEventStream = Box::pin(create_sse_stream(
        receiver,
        response_id,
        req.model.clone(),
        created_at,
    ));

    Sse::new(sse_stream).keep_alive(KeepAlive::default())
}

/// Create the SSE stream from broadcast receiver.
fn create_sse_stream(
    receiver: broadcast::Receiver<StreamEvent>,
    response_id: String,
    model: String,
    created_at: i64,
) -> impl Stream<Item = Result<Event, Infallible>> + Send {
    // First, emit the response.created event
    let created_event = ResponseCreatedEvent {
        id: response_id.clone(),
        object: "response",
        model: model.clone(),
        created_at,
    };

    let initial = stream::once(async move {
        Ok::<_, Infallible>(
            Event::default()
                .event("response.created")
                .json_data(created_event)
                .unwrap(),
        )
    });

    // State for unfold: (receiver, response_id, terminated)
    let state = (receiver, response_id, false);

    // Use unfold to properly manage async state with termination
    let event_stream = stream::unfold(
        state,
        |(mut receiver, response_id, terminated)| async move {
            if terminated {
                return None;
            }

            // Use the receiver directly instead of BroadcastStream
            match receiver.recv().await {
                Ok(event) => {
                    let is_terminal = matches!(
                        &event,
                        StreamEvent::StatusUpdate { status, .. }
                            if status.is_terminal()
                    );
                    let sse_event = stream_event_to_sse(event, &response_id);
                    Some((sse_event, (receiver, response_id, is_terminal)))
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "Broadcast stream lagged, skipping events");
                    // Continue receiving after lag
                    Some((
                        Ok(Event::default().comment(format!("skipped {} events", n))),
                        (receiver, response_id, false),
                    ))
                }
                Err(broadcast::error::RecvError::Closed) => {
                    // Channel closed, stream ends
                    None
                }
            }
        },
    );

    // Chain initial event with the broadcast stream
    initial.chain(event_stream)
}

/// Convert a StreamEvent to an SSE Event.
fn stream_event_to_sse(event: StreamEvent, response_id: &str) -> Result<Event, Infallible> {
    match event {
        StreamEvent::OutputChunk {
            seq: _,
            content,
            is_final: _,
            timestamp_ms: _,
        } => {
            let delta_event = OutputTextDeltaEvent {
                response_id: response_id.to_string(),
                output_index: 0,
                delta: DeltaContent {
                    content_type: "text/plain".to_string(),
                    text: content,
                },
            };
            Ok(Event::default()
                .event("response.output_text.delta")
                .json_data(delta_event)
                .unwrap())
        }
        StreamEvent::StatusUpdate {
            status,
            error_message,
            timestamp_ms: _,
        } => {
            if status == RunStatus::Completed {
                let completed_event = ResponseCompletedEvent {
                    id: response_id.to_string(),
                    status: "completed".to_string(),
                    output: vec![], // Output already streamed via deltas
                    usage: None,    // TODO: Track token usage
                };
                Ok(Event::default()
                    .event("response.completed")
                    .json_data(completed_event)
                    .unwrap())
            } else if status == RunStatus::Failed || status == RunStatus::Cancelled {
                let status_str = if status == RunStatus::Failed {
                    "failed"
                } else {
                    "cancelled"
                };
                let failed_event = ResponseFailedEvent {
                    id: response_id.to_string(),
                    status: status_str.to_string(),
                    error: ErrorObject {
                        message: error_message.unwrap_or_else(|| "Unknown error".to_string()),
                        error_type: "agent_error".to_string(),
                        code: "execution_failed".to_string(),
                        param: None,
                    },
                };
                Ok(Event::default()
                    .event("response.failed")
                    .json_data(failed_event)
                    .unwrap())
            } else {
                // Running or other status - emit as comment (no-op for client)
                Ok(Event::default().comment(format!("status: {:?}", status)))
            }
        }
    }
}

/// Create a non-streaming response (original implementation).
async fn create_non_streaming_response(
    state: Arc<AppState>,
    req: CreateResponseRequest,
) -> impl IntoResponse {
    // Map model to agent_name (direct mapping for MVP)
    let agent_name = resolve_agent_name(&req.model);

    // Build input_json from request
    let input_json = build_input_json(&req);

    // Create task
    let mut task = Task::new(&agent_name, &input_json, "http-api");
    for (k, v) in &req.metadata {
        task.labels.insert(k.clone(), v.clone());
    }
    task.labels
        .insert("source".to_string(), "openai_api".to_string());

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
            run_id
        }
        Err(e) => {
            warn!(task_id = %task_id, error = %e, "Failed to assign task");
            return ApiError::NoWorkersAvailable { agent: agent_name }.into_response();
        }
    };

    // Wait for task completion (poll with timeout)
    let timeout = Duration::from_secs(300); // 5 minute timeout
    let poll_interval = Duration::from_millis(100);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            warn!(task_id = %task_id, "Task timed out");
            return ApiError::TaskTimeout.into_response();
        }

        // Check task status
        let task_opt = state.tasks.read().await.get(&task_id).cloned();
        let task = match task_opt {
            Some(t) => t,
            None => {
                return ApiError::Internal {
                    message: "Task disappeared".to_string(),
                }
                .into_response();
            }
        };

        if task.is_terminal() {
            // Task completed, build response
            let response = build_response(&state, &task, &req.model, Some(&run_id)).await;
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
            param: None,
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
