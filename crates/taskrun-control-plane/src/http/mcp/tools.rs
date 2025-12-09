//! MCP tool handler implementations.

use std::sync::Arc;

use axum::{extract::State, Json};
use tracing::{info, warn};

use taskrun_core::{ChatRole, RunEventType, Task, TaskId};
use taskrun_proto::pb::run_server_message::Payload as ServerPayload;
use taskrun_proto::pb::{ContinueRun, RunServerMessage};

use crate::scheduler::Scheduler;
use crate::state::AppState;

use super::types::*;

// ============================================================================
// list_workers
// ============================================================================

/// List all connected workers.
pub async fn list_workers(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest<ListWorkersParams>>,
) -> Json<McpResponse<ListWorkersResult>> {
    let workers = state.workers.read().await;

    let workers_list: Vec<WorkerInfo> = workers
        .values()
        .filter(|w| {
            // Apply optional agent filter
            if let Some(ref agent) = request.params.agent {
                w.info.supports_agent(agent)
            } else {
                true
            }
        })
        .map(|w| WorkerInfo {
            worker_id: w.info.worker_id.as_str().to_string(),
            hostname: w.info.hostname.clone(),
            status: format!("{:?}", w.status).to_uppercase(),
            agents: w.info.agents.iter().map(|a| a.name.clone()).collect(),
            active_runs: w.active_runs,
            max_concurrent_runs: w.max_concurrent_runs,
        })
        .collect();

    Json(McpResponse::ok(ListWorkersResult {
        workers: workers_list,
    }))
}

// ============================================================================
// start_new_task
// ============================================================================

/// Start a new task on an available worker.
pub async fn start_new_task(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest<StartNewTaskParams>>,
) -> Json<McpResponse<StartNewTaskResult>> {
    let params = request.params;

    // Convert input to JSON string
    let input_json = match &params.input {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    // Check if any worker supports this agent
    if !state.has_agent(&params.agent_name).await {
        return Json(McpResponse::err(
            "NO_AGENT",
            &format!("No worker supports agent: {}", params.agent_name),
        ));
    }

    // Create task
    let mut task = Task::new(&params.agent_name, &input_json, "mcp");

    // Add metadata
    task.labels.insert("source".to_string(), "mcp".to_string());
    for (key, value) in params.metadata {
        task.labels.insert(key, value);
    }

    let task_id = task.id.clone();

    // Store task
    state.tasks.write().await.insert(task_id.clone(), task);

    info!(
        task_id = %task_id,
        agent = %params.agent_name,
        "Created task via MCP"
    );

    // Schedule task
    let scheduler = Scheduler::new(state.clone());
    match scheduler.assign_task(&task_id).await {
        Ok(run_id) => {
            info!(
                task_id = %task_id,
                run_id = %run_id,
                "Task assigned to worker"
            );
            Json(McpResponse::ok(StartNewTaskResult {
                task_id: task_id.as_str().to_string(),
                run_id: run_id.as_str().to_string(),
                status: "running".to_string(),
            }))
        }
        Err(e) => {
            warn!(task_id = %task_id, error = %e, "Failed to schedule task");
            Json(McpResponse::err("SCHEDULE_FAILED", &e.to_string()))
        }
    }
}

// ============================================================================
// read_task
// ============================================================================

/// Read task status, output, events, and chat history.
pub async fn read_task(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest<ReadTaskParams>>,
) -> Json<McpResponse<ReadTaskResult>> {
    let task_id = TaskId::new(&request.params.task_id);

    // Verify task exists
    {
        let tasks = state.tasks.read().await;
        if !tasks.contains_key(&task_id) {
            return Json(McpResponse::err(
                "NOT_FOUND",
                &format!("Task not found: {}", request.params.task_id),
            ));
        }
    }

    // Get output
    let output = state.get_output_by_task(&task_id).await;

    // Get events
    let events: Vec<TaskEvent> = state
        .get_events_by_task(&task_id)
        .await
        .into_iter()
        .map(|e| TaskEvent {
            event_type: event_type_to_string(&e.event_type),
            timestamp_ms: e.timestamp_ms,
            metadata: e.metadata,
        })
        .collect();

    // Get chat messages
    let chat_messages: Vec<ChatMessageInfo> = state
        .get_chat_messages_by_task(&task_id)
        .await
        .into_iter()
        .map(|m| ChatMessageInfo {
            role: chat_role_to_string(&m.role),
            content: m.content,
            timestamp_ms: m.timestamp_ms,
        })
        .collect();

    // Get session_id from SessionInitialized event metadata
    let session_id = events.iter().find_map(|e| {
        if e.event_type == "session_initialized" {
            e.metadata.get("session_id").cloned()
        } else {
            None
        }
    });

    // Re-acquire task for final read
    let tasks = state.tasks.read().await;
    let task = tasks.get(&task_id).unwrap();

    Json(McpResponse::ok(ReadTaskResult {
        task_id: task_id.as_str().to_string(),
        status: format!("{:?}", task.status).to_lowercase(),
        agent_name: task.agent_name.clone(),
        created_at: task.created_at.to_rfc3339(),
        output,
        events,
        chat_messages,
        session_id,
    }))
}

// ============================================================================
// continue_task
// ============================================================================

/// Continue a task with a follow-up message.
pub async fn continue_task(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest<ContinueTaskParams>>,
) -> Json<McpResponse<ContinueTaskResult>> {
    let task_id = TaskId::new(&request.params.task_id);

    // Get task and its latest run
    let (run_id, worker_id) = {
        let tasks = state.tasks.read().await;
        let task = match tasks.get(&task_id) {
            Some(t) => t,
            None => {
                return Json(McpResponse::err(
                    "NOT_FOUND",
                    &format!("Task not found: {}", request.params.task_id),
                ));
            }
        };

        // Get the latest run
        let latest_run = match task.runs.last() {
            Some(r) => r,
            None => {
                return Json(McpResponse::err("NO_RUN", "Task has no runs to continue"));
            }
        };

        (latest_run.run_id.clone(), latest_run.worker_id.clone())
    };

    // Send ContinueRun message to worker
    let workers = state.workers.read().await;
    let worker = match workers.get(&worker_id) {
        Some(w) => w,
        None => {
            return Json(McpResponse::err(
                "WORKER_DISCONNECTED",
                &format!("Worker {} is no longer connected", worker_id),
            ));
        }
    };

    let continue_msg = RunServerMessage {
        payload: Some(ServerPayload::ContinueRun(ContinueRun {
            run_id: run_id.as_str().to_string(),
            message: request.params.message.clone(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        })),
    };

    if worker.tx.send(continue_msg).await.is_err() {
        return Json(McpResponse::err(
            "SEND_FAILED",
            "Failed to send continue message to worker",
        ));
    }

    info!(
        task_id = %task_id,
        run_id = %run_id,
        "Sent continue message to worker via MCP"
    );

    Json(McpResponse::ok(ContinueTaskResult {
        task_id: task_id.as_str().to_string(),
        run_id: run_id.as_str().to_string(),
        status: "running".to_string(),
    }))
}

// ============================================================================
// Helpers
// ============================================================================

fn event_type_to_string(event_type: &RunEventType) -> String {
    match event_type {
        RunEventType::ExecutionStarted => "execution_started",
        RunEventType::SessionInitialized => "session_initialized",
        RunEventType::ToolRequested => "tool_requested",
        RunEventType::ToolCompleted => "tool_completed",
        RunEventType::OutputGenerated => "output_generated",
        RunEventType::ExecutionCompleted => "execution_completed",
        RunEventType::ExecutionFailed => "execution_failed",
    }
    .to_string()
}

fn chat_role_to_string(role: &ChatRole) -> String {
    match role {
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::System => "system",
    }
    .to_string()
}
