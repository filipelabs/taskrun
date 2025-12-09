//! MCP (Model Context Protocol) server implementation.
//!
//! Provides MCP tools for interacting with TaskRun:
//! - `list_workers` - List connected workers
//! - `start_new_task` - Create and start a new task
//! - `get_task` - Get task details including status, input, output, and run history
//! - `continue_task` - Continue an existing task with a follow-up message

use std::sync::Arc;

use axum::Router;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData as McpError, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::info;

use taskrun_control_plane::state::{AppState, UiNotification};
use taskrun_control_plane::Scheduler;
use taskrun_core::{ChatRole, Task};
use taskrun_proto::pb::run_server_message::Payload as ServerPayload;
use taskrun_proto::pb::{ContinueRun, RunServerMessage};

/// Convert ChatRole to string.
fn chat_role_to_string(role: &ChatRole) -> String {
    match role {
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::System => "system",
    }
    .to_string()
}

/// MCP server for TaskRun operations.
#[derive(Clone)]
pub struct TaskRunMcpServer {
    state: Arc<AppState>,
    tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
}

// ============================================================================
// Tool Parameter Types
// ============================================================================

/// Parameters for list_workers tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListWorkersParams {
    /// Optional filter by agent name.
    #[serde(default)]
    pub agent: Option<String>,
}

/// Parameters for start_new_task tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartNewTaskParams {
    /// Name of the agent to run the task.
    pub agent_name: String,

    /// Input for the task (plain text or JSON string).
    pub input: String,
}

/// Parameters for continue_task tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContinueTaskParams {
    /// Task ID to continue.
    pub task_id: String,

    /// Follow-up message to send.
    pub message: String,
}

/// Parameters for get_task tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetTaskParams {
    /// Task ID to retrieve.
    pub task_id: String,
}

// ============================================================================
// Response Types
// ============================================================================

/// Worker information returned by list_workers.
#[derive(Debug, Serialize)]
pub struct WorkerInfo {
    pub worker_id: String,
    pub hostname: String,
    pub status: String,
    pub agents: Vec<String>,
    pub active_runs: u32,
    pub max_concurrent_runs: u32,
}

/// Result of starting a new task.
#[derive(Debug, Serialize)]
pub struct StartTaskResult {
    pub task_id: String,
    pub run_id: String,
    pub status: String,
}

/// Result of continuing a task.
#[derive(Debug, Serialize)]
pub struct ContinueTaskResult {
    pub task_id: String,
    pub run_id: String,
    pub status: String,
}

/// Chat message in the conversation.
#[derive(Debug, Serialize)]
pub struct ChatMessageInfo {
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

/// Task details returned by get_task.
#[derive(Debug, Serialize)]
pub struct TaskDetails {
    pub task_id: String,
    pub agent_name: String,
    pub status: String,
    pub input: String,
    pub created_at: String,

    /// Output from the task (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,

    /// Chat messages in the conversation.
    pub chat_messages: Vec<ChatMessageInfo>,
}

// ============================================================================
// Tool Implementations
// ============================================================================

#[tool_router]
impl TaskRunMcpServer {
    /// Create a new MCP server with the given AppState.
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    /// List all connected workers and their capabilities.
    #[tool(description = "List all connected workers and their capabilities. Optionally filter by agent name.")]
    async fn list_workers(
        &self,
        Parameters(params): Parameters<ListWorkersParams>,
    ) -> Result<CallToolResult, McpError> {
        let workers = self.state.workers.read().await;

        let workers_list: Vec<WorkerInfo> = workers
            .values()
            .filter(|w| {
                if let Some(ref agent) = params.agent {
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

        let response = serde_json::to_string_pretty(&workers_list)
            .unwrap_or_else(|_| "[]".to_string());

        info!(worker_count = workers_list.len(), "Listed workers via MCP");

        Ok(CallToolResult::success(vec![Content::text(response)]))
    }

    /// Start a new task on an available worker.
    #[tool(description = "Start a new task on an available worker. Requires an agent name and input text.")]
    async fn start_new_task(
        &self,
        Parameters(params): Parameters<StartNewTaskParams>,
    ) -> Result<CallToolResult, McpError> {
        // Check if any worker supports this agent
        if !self.state.has_agent(&params.agent_name).await {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No worker supports agent: {}",
                params.agent_name
            ))]));
        }

        // Create task
        let task = Task::new(&params.agent_name, &params.input, "mcp");
        let task_id = task.id.clone();

        // Store task
        self.state
            .tasks
            .write()
            .await
            .insert(task_id.clone(), task);

        // Notify UI
        self.state.notify_ui(UiNotification::TaskCreated {
            task_id: task_id.clone(),
            agent: params.agent_name.clone(),
        });

        info!(
            task_id = %task_id,
            agent = %params.agent_name,
            "Created task via MCP"
        );

        // Schedule task
        let scheduler = Scheduler::new(self.state.clone());
        match scheduler.assign_task(&task_id).await {
            Ok(run_id) => {
                info!(
                    task_id = %task_id,
                    run_id = %run_id,
                    "Task assigned to worker"
                );

                let result = StartTaskResult {
                    task_id: task_id.as_str().to_string(),
                    run_id: run_id.as_str().to_string(),
                    status: "running".to_string(),
                };

                let response = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| "{}".to_string());

                Ok(CallToolResult::success(vec![Content::text(response)]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to schedule task: {}",
                    e
                ))]))
            }
        }
    }

    /// Get details about a task.
    #[tool(description = "Get details about a task including its status, input, output, and run history.")]
    async fn get_task(
        &self,
        Parameters(params): Parameters<GetTaskParams>,
    ) -> Result<CallToolResult, McpError> {
        let task_id = taskrun_core::TaskId::new(&params.task_id);

        // Verify task exists
        {
            let tasks = self.state.tasks.read().await;
            if !tasks.contains_key(&task_id) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Task not found: {}",
                    params.task_id
                ))]));
            }
        }

        // Get output
        let output = self.state.get_output_by_task(&task_id).await;

        // Get chat messages
        let chat_messages: Vec<ChatMessageInfo> = self
            .state
            .get_chat_messages_by_task(&task_id)
            .await
            .into_iter()
            .map(|m| {
                let timestamp = chrono::DateTime::from_timestamp_millis(m.timestamp_ms)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default();
                ChatMessageInfo {
                    role: chat_role_to_string(&m.role),
                    content: m.content,
                    timestamp,
                }
            })
            .collect();

        // Re-acquire task for final read
        let tasks = self.state.tasks.read().await;
        let task = tasks.get(&task_id).unwrap();

        let details = TaskDetails {
            task_id: task.id.as_str().to_string(),
            agent_name: task.agent_name.clone(),
            status: format!("{:?}", task.status).to_lowercase(),
            input: task.input_json.clone(),
            created_at: task.created_at.to_rfc3339(),
            output,
            chat_messages,
        };

        let response =
            serde_json::to_string_pretty(&details).unwrap_or_else(|_| "{}".to_string());

        info!(task_id = %task_id, "Retrieved task details via MCP");

        Ok(CallToolResult::success(vec![Content::text(response)]))
    }

    /// Continue an existing task with a follow-up message.
    #[tool(description = "Continue an existing task by sending a follow-up message. Requires the task ID and message.")]
    async fn continue_task(
        &self,
        Parameters(params): Parameters<ContinueTaskParams>,
    ) -> Result<CallToolResult, McpError> {
        let task_id = taskrun_core::TaskId::new(&params.task_id);

        // Get task and its latest run
        let (run_id, worker_id) = {
            let tasks = self.state.tasks.read().await;
            let task = match tasks.get(&task_id) {
                Some(t) => t,
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Task not found: {}",
                        params.task_id
                    ))]));
                }
            };

            // Get the latest run
            let latest_run = match task.runs.last() {
                Some(r) => r,
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(
                        "Task has no runs to continue".to_string(),
                    )]));
                }
            };

            (latest_run.run_id.clone(), latest_run.worker_id.clone())
        };

        // Send ContinueRun message to worker
        let workers = self.state.workers.read().await;
        let worker = match workers.get(&worker_id) {
            Some(w) => w,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Worker {} is no longer connected",
                    worker_id
                ))]));
            }
        };

        let continue_msg = RunServerMessage {
            payload: Some(ServerPayload::ContinueRun(ContinueRun {
                run_id: run_id.as_str().to_string(),
                message: params.message.clone(),
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
            })),
        };

        if worker.tx.send(continue_msg).await.is_err() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Failed to send continue message to worker".to_string(),
            )]));
        }

        info!(
            task_id = %task_id,
            run_id = %run_id,
            "Sent continue message to worker via MCP"
        );

        let result = ContinueTaskResult {
            task_id: task_id.as_str().to_string(),
            run_id: run_id.as_str().to_string(),
            status: "running".to_string(),
        };

        let response =
            serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string());

        Ok(CallToolResult::success(vec![Content::text(response)]))
    }
}

// ============================================================================
// Server Handler Implementation
// ============================================================================

#[tool_handler]
impl ServerHandler for TaskRunMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: rmcp::model::Implementation {
                name: "taskrun-server".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: None,
                website_url: None,
                icons: None,
            },
            instructions: Some(
                "TaskRun MCP Server - Control AI agent tasks on remote workers. \
                 Use list_workers to see available workers, start_new_task to create tasks, \
                 and continue_task to send follow-up messages."
                    .to_string(),
            ),
        }
    }
}

// ============================================================================
// HTTP Server Setup
// ============================================================================

/// Create an axum Router for the MCP HTTP server.
///
/// This router handles MCP protocol requests over HTTP using the Streamable HTTP transport.
/// Mount this at `/mcp` on your existing HTTP server or run it standalone.
pub fn create_mcp_router(state: Arc<AppState>, ct: CancellationToken) -> Router {
    let state_clone = state.clone();
    let service = StreamableHttpService::new(
        move || Ok(TaskRunMcpServer::new(state_clone.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig {
            cancellation_token: ct,
            ..Default::default()
        },
    );

    info!("MCP server initialized with Streamable HTTP transport");

    Router::new().nest_service("/mcp", service)
}
