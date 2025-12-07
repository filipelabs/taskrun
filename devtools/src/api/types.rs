//! API response types matching the control plane.

use serde::{Deserialize, Serialize};

/// Response for a single worker.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerResponse {
    pub worker_id: String,
    pub hostname: String,
    pub version: String,
    pub status: String,
    pub active_runs: u32,
    pub max_concurrent_runs: u32,
    pub last_heartbeat: String,
    pub agents: Vec<AgentResponse>,
}

/// Agent information in worker response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentResponse {
    pub name: String,
    pub description: String,
    pub backends: Vec<BackendResponse>,
}

/// Model backend information.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackendResponse {
    pub provider: String,
    pub model_name: String,
}

/// Health check response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthResponse {
    pub status: String,
}

/// Run event response from the control plane.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventResponse {
    pub id: String,
    pub run_id: String,
    pub task_id: String,
    pub event_type: String,
    pub timestamp_ms: i64,
    pub metadata: std::collections::HashMap<String, String>,
}

impl EventResponse {
    /// Get a display-friendly name for the event type.
    pub fn event_type_display(&self) -> &str {
        match self.event_type.as_str() {
            "execution_started" => "Execution Started",
            "session_initialized" => "Session Initialized",
            "tool_requested" => "Tool Requested",
            "tool_completed" => "Tool Completed",
            "output_generated" => "Output Generated",
            "execution_completed" => "Execution Completed",
            "execution_failed" => "Execution Failed",
            _ => &self.event_type,
        }
    }

    /// Get an icon/emoji for the event type.
    pub fn event_icon(&self) -> &str {
        match self.event_type.as_str() {
            "execution_started" => "▶",
            "session_initialized" => "🔗",
            "tool_requested" => "🔧",
            "tool_completed" => "✓",
            "output_generated" => "📝",
            "execution_completed" => "✅",
            "execution_failed" => "❌",
            _ => "•",
        }
    }

    /// Get the CSS color class for the event type.
    pub fn event_color(&self) -> &str {
        match self.event_type.as_str() {
            "execution_started" => "text-blue-400",
            "session_initialized" => "text-purple-400",
            "tool_requested" => "text-yellow-400",
            "tool_completed" => "text-green-400",
            "output_generated" => "text-gray-400",
            "execution_completed" => "text-green-500",
            "execution_failed" => "text-red-500",
            _ => "text-gray-400",
        }
    }
}

/// Task output response from the control plane.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputResponse {
    pub task_id: String,
    pub output: Option<String>,
}

// ============================================================================
// SSE Streaming Types (OpenAI-compatible)
// ============================================================================

/// SSE event: response.created
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponseCreatedEvent {
    pub id: String,
    pub object: String,
    pub model: String,
    pub created_at: i64,
}

/// SSE event: response.output_text.delta
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputTextDeltaEvent {
    pub response_id: String,
    pub output_index: u32,
    pub delta: DeltaContent,
}

/// Delta content in streaming events.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeltaContent {
    pub content_type: String,
    pub text: String,
}

/// SSE event: response.completed
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponseCompletedEvent {
    pub id: String,
    pub status: String,
}

/// SSE event: response.failed
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponseFailedEvent {
    pub id: String,
    pub status: String,
    pub error: Option<ResponseError>,
}

/// Error details in failed response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponseError {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub code: Option<String>,
}

/// Parsed SSE event from the stream.
#[derive(Debug, Clone)]
pub enum SseEvent {
    Created(ResponseCreatedEvent),
    Delta(OutputTextDeltaEvent),
    Completed(ResponseCompletedEvent),
    Failed(ResponseFailedEvent),
    Unknown(String, String), // (event_type, data)
}

impl SseEvent {
    /// Parse an SSE event from event type and JSON data.
    pub fn parse(event_type: &str, data: &str) -> Option<Self> {
        match event_type {
            "response.created" => {
                serde_json::from_str::<ResponseCreatedEvent>(data)
                    .ok()
                    .map(SseEvent::Created)
            }
            "response.output_text.delta" => {
                serde_json::from_str::<OutputTextDeltaEvent>(data)
                    .ok()
                    .map(SseEvent::Delta)
            }
            "response.completed" => {
                serde_json::from_str::<ResponseCompletedEvent>(data)
                    .ok()
                    .map(SseEvent::Completed)
            }
            "response.failed" => {
                serde_json::from_str::<ResponseFailedEvent>(data)
                    .ok()
                    .map(SseEvent::Failed)
            }
            _ => Some(SseEvent::Unknown(event_type.to_string(), data.to_string())),
        }
    }
}

/// Parsed metrics from Prometheus format.
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    pub workers_idle: u32,
    pub workers_busy: u32,
    pub workers_draining: u32,
    pub workers_error: u32,
    pub tasks_pending: u32,
    pub tasks_running: u32,
    pub tasks_completed: u32,
    pub tasks_failed: u32,
    pub tasks_cancelled: u32,
}

impl Metrics {
    /// Parse Prometheus text format into Metrics struct.
    pub fn from_prometheus(text: &str) -> Self {
        let mut metrics = Self::default();

        for line in text.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            // Parse lines like: taskrun_workers_connected{status="idle"} 1
            if let Some((key, value)) = line.rsplit_once(' ') {
                if let Ok(v) = value.parse::<u32>() {
                    if key.contains("workers_connected") {
                        if key.contains("idle") {
                            metrics.workers_idle = v;
                        } else if key.contains("busy") {
                            metrics.workers_busy = v;
                        } else if key.contains("draining") {
                            metrics.workers_draining = v;
                        } else if key.contains("error") {
                            metrics.workers_error = v;
                        }
                    } else if key.contains("tasks_total") {
                        if key.contains("pending") {
                            metrics.tasks_pending = v;
                        } else if key.contains("running") {
                            metrics.tasks_running = v;
                        } else if key.contains("completed") {
                            metrics.tasks_completed = v;
                        } else if key.contains("failed") {
                            metrics.tasks_failed = v;
                        } else if key.contains("cancelled") {
                            metrics.tasks_cancelled = v;
                        }
                    }
                }
            }
        }

        metrics
    }

    /// Total number of workers.
    pub fn total_workers(&self) -> u32 {
        self.workers_idle + self.workers_busy + self.workers_draining + self.workers_error
    }

    /// Total number of tasks.
    pub fn total_tasks(&self) -> u32 {
        self.tasks_pending + self.tasks_running + self.tasks_completed + self.tasks_failed + self.tasks_cancelled
    }
}
