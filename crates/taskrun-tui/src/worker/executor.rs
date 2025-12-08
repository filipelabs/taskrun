//! Agent execution via Claude Code SDK.
//!
//! Adapted from taskrun-worker for use in the TUI.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};
use taskrun_claude_sdk::{
    ClaudeExecutor, ClaudeMessage, ContentDelta, ContentItem, ControlHandler, PermissionMode,
    PermissionResult, SdkError, StreamEvent,
};
use taskrun_core::{RunEvent, RunId, TaskId};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::connection::ConnectionConfig;

/// Errors that can occur during agent execution.
#[derive(Debug, Error)]
#[allow(dead_code)] // Some variants are for API completeness
pub enum ExecutorError {
    #[error("Claude CLI not found at '{0}'. Ensure Claude Code is installed.")]
    ClaudeNotFound(String),

    #[error("Failed to spawn Claude process: {0}")]
    SpawnError(#[from] std::io::Error),

    #[error("Claude process exited with error: {0}")]
    ProcessError(String),

    #[error("Unknown agent: {0}")]
    UnknownAgent(String),

    #[error("SDK error: {0}")]
    SdkError(String),
}

/// Output chunk from Claude Code execution.
#[derive(Debug, Clone)]
pub struct OutputChunk {
    pub content: String,
    pub is_final: bool,
}

/// Handler that streams Claude messages as output chunks and emits events.
struct StreamingHandler {
    output_tx: mpsc::Sender<OutputChunk>,
    event_tx: mpsc::Sender<RunEvent>,
    run_id: RunId,
    task_id: TaskId,
    session_id: Arc<Mutex<Option<String>>>,
    model_used: Arc<Mutex<Option<String>>>,
}

impl StreamingHandler {
    fn new(
        output_tx: mpsc::Sender<OutputChunk>,
        event_tx: mpsc::Sender<RunEvent>,
        run_id: RunId,
        task_id: TaskId,
    ) -> Self {
        Self {
            output_tx,
            event_tx,
            run_id,
            task_id,
            session_id: Arc::new(Mutex::new(None)),
            model_used: Arc::new(Mutex::new(None)),
        }
    }

    fn session_id(&self) -> Option<String> {
        self.session_id.lock().unwrap().clone()
    }

    fn model_used(&self) -> Option<String> {
        self.model_used.lock().unwrap().clone()
    }

    async fn emit_event(&self, event: RunEvent) {
        if self.event_tx.send(event).await.is_err() {
            warn!("Failed to send event - receiver dropped");
        }
    }
}

#[async_trait]
impl ControlHandler for StreamingHandler {
    async fn on_can_use_tool(
        &self,
        tool_name: String,
        input: Value,
    ) -> Result<PermissionResult, SdkError> {
        info!(tool = %tool_name, "Auto-approving tool use");
        Ok(PermissionResult::Allow {
            updated_input: input,
            updated_permissions: None,
        })
    }

    async fn on_hook_callback(
        &self,
        callback_id: String,
        _input: Value,
        _tool_use_id: Option<String>,
    ) -> Result<Value, SdkError> {
        info!(callback = %callback_id, "Auto-approving hook callback");
        Ok(json!({
            "hookSpecificOutput": {
                "permissionDecision": "allow"
            }
        }))
    }

    async fn on_message(&self, message: ClaudeMessage) -> Result<(), SdkError> {
        // Log message type first
        let msg_type = match &message {
            ClaudeMessage::System { .. } => "System",
            ClaudeMessage::Assistant { .. } => "Assistant",
            ClaudeMessage::User { .. } => "User",
            ClaudeMessage::ToolUse { .. } => "ToolUse",
            ClaudeMessage::ToolResult { .. } => "ToolResult",
            ClaudeMessage::StreamEvent { .. } => "StreamEvent",
            ClaudeMessage::Result { .. } => "Result",
            ClaudeMessage::ControlRequest { .. } => "ControlRequest",
            ClaudeMessage::Unknown(_) => "Unknown",
        };
        debug!(message_type = msg_type, "StreamingHandler received message");

        match message {
            ClaudeMessage::System {
                session_id, model, ..
            } => {
                // Capture session ID and model for future use
                info!(session_id = ?session_id, model = ?model, "System message received");

                let sid_clone = session_id.clone();
                let model_clone = model.clone();

                if let Some(sid) = session_id {
                    info!(session_id = %sid, "Captured session ID");
                    *self.session_id.lock().unwrap() = Some(sid);
                }
                if let Some(m) = model {
                    info!(model = %m, "Captured model used");
                    *self.model_used.lock().unwrap() = Some(m);
                }

                // Emit SessionInitialized event
                self.emit_event(RunEvent::session_initialized(
                    self.run_id.clone(),
                    self.task_id.clone(),
                    sid_clone,
                    model_clone,
                ))
                .await;
            }
            ClaudeMessage::Assistant { message, .. } => {
                // Extract text content and stream it
                debug!(
                    content_count = message.content.len(),
                    "Assistant message received"
                );
                for content in message.content {
                    if let ContentItem::Text { text } = content {
                        debug!(text_len = text.len(), "Streaming assistant text chunk");
                        let chunk = OutputChunk {
                            content: text,
                            is_final: false,
                        };
                        if self.output_tx.send(chunk).await.is_err() {
                            warn!("Failed to send output chunk - receiver dropped");
                        }
                    }
                }
            }
            ClaudeMessage::StreamEvent { event, .. } => {
                // Handle streaming delta events for real-time token output
                if let StreamEvent::ContentBlockDelta {
                    delta: ContentDelta::TextDelta { text },
                    index,
                } = event
                {
                    debug!(
                        text_len = text.len(),
                        block_index = index,
                        "Streaming text delta"
                    );
                    let chunk = OutputChunk {
                        content: text,
                        is_final: false,
                    };
                    if self.output_tx.send(chunk).await.is_err() {
                        warn!("Failed to send output chunk - receiver dropped");
                    }
                }
            }
            ClaudeMessage::Result {
                is_error,
                duration_ms,
                error,
                ..
            } => {
                info!(
                    is_error = ?is_error,
                    duration_ms = ?duration_ms,
                    "Execution result received"
                );

                // Emit ExecutionCompleted or ExecutionFailed event
                if is_error == Some(true) {
                    self.emit_event(RunEvent::execution_failed(
                        self.run_id.clone(),
                        self.task_id.clone(),
                        error,
                    ))
                    .await;
                } else {
                    self.emit_event(RunEvent::execution_completed(
                        self.run_id.clone(),
                        self.task_id.clone(),
                        duration_ms.map(|d| d as i64),
                    ))
                    .await;
                }
            }
            ClaudeMessage::ToolUse { tool_name, .. } => {
                debug!(tool = %tool_name, "Tool use message");

                // Emit ToolRequested event
                self.emit_event(RunEvent::tool_requested(
                    self.run_id.clone(),
                    self.task_id.clone(),
                    &tool_name,
                ))
                .await;
            }
            ClaudeMessage::ToolResult { is_error, .. } => {
                debug!(is_error = ?is_error, "Tool result message");

                // Emit ToolCompleted event
                self.emit_event(RunEvent::tool_completed(
                    self.run_id.clone(),
                    self.task_id.clone(),
                    is_error.unwrap_or(false),
                ))
                .await;
            }
            ClaudeMessage::Unknown(ref value) => {
                // Log the full unknown message for debugging
                let full_json = serde_json::to_string(value)
                    .unwrap_or_else(|_| "failed to serialize".to_string());
                // Check if this is a control_response (expected)
                if value.get("type").and_then(|t| t.as_str()) == Some("control_response") {
                    debug!(
                        len = full_json.len(),
                        "Received control_response (expected)"
                    );
                } else {
                    // Log full message for unexpected types
                    warn!(full_message = %full_json, "Received unexpected Unknown message type");
                }
            }
            _ => {
                debug!("Ignoring other message type");
            }
        }
        Ok(())
    }
}

/// Executes agents via Claude Code SDK.
#[derive(Clone)]
pub struct ClaudeCodeExecutor {
    /// Worker configuration including claude path and tool permissions.
    config: Arc<ConnectionConfig>,
}

impl ClaudeCodeExecutor {
    /// Create a new executor with the given configuration.
    pub fn new(config: Arc<ConnectionConfig>) -> Self {
        Self { config }
    }

    /// Execute an agent with the given input, streaming output and events via channels.
    ///
    /// Returns when execution completes (successfully or with error).
    pub async fn execute(
        &self,
        agent_name: &str,
        input_json: &str,
        output_tx: mpsc::Sender<OutputChunk>,
        event_tx: mpsc::Sender<RunEvent>,
        run_id: RunId,
        task_id: TaskId,
    ) -> Result<ExecutionResult, ExecutorError> {
        info!(
            agent = %agent_name,
            claude_path = %self.config.claude_path,
            input_len = input_json.len(),
            allowed_tools = ?self.config.allowed_tools,
            denied_tools = ?self.config.denied_tools,
            "Starting agent execution"
        );

        // Emit ExecutionStarted event
        if event_tx
            .send(RunEvent::execution_started(run_id.clone(), task_id.clone()))
            .await
            .is_err()
        {
            warn!("Failed to send ExecutionStarted event");
        }

        // Build the prompt based on agent type
        let prompt = self.build_prompt(agent_name, input_json)?;
        info!(prompt_len = prompt.len(), "Built prompt for agent");

        info!(agent = %agent_name, "Creating Claude Code SDK executor");
        debug!(prompt = %prompt, "Full prompt");

        // Create SDK executor with bypass permissions (auto-approve all)
        let mut sdk_executor = ClaudeExecutor::new(&self.config.claude_path)
            .with_permission_mode(PermissionMode::BypassPermissions);

        // Apply tool permissions from config
        if let Some(ref allowed) = self.config.allowed_tools {
            sdk_executor = sdk_executor.with_allowed_tools(allowed.clone());
            info!(allowed_tools = ?allowed, "Applying allowed tools filter");
        }
        if let Some(ref denied) = self.config.denied_tools {
            sdk_executor = sdk_executor.with_disallowed_tools(denied.clone());
            info!(denied_tools = ?denied, "Applying denied tools filter");
        }

        // Create streaming handler with event support
        let handler = Arc::new(StreamingHandler::new(
            output_tx.clone(),
            event_tx,
            run_id,
            task_id,
        ));

        // Execute via SDK in the configured working directory
        let working_dir = Path::new(&self.config.working_dir);
        let result = sdk_executor
            .execute(working_dir, &prompt, handler.clone())
            .await
            .map_err(|e| ExecutorError::SdkError(e.to_string()))?;

        // Send final marker
        let _ = output_tx
            .send(OutputChunk {
                content: String::new(),
                is_final: true,
            })
            .await;

        let session_id = handler.session_id();
        // Use the real model from Claude's System message, fallback to SDK's placeholder
        let model_used = handler.model_used().unwrap_or(result.model_used);
        info!(
            session_id = ?session_id,
            model = %model_used,
            "Claude Code execution completed"
        );

        Ok(ExecutionResult {
            model_used,
            provider: "anthropic".to_string(),
            session_id,
        })
    }

    /// Execute a follow-up message in an existing session.
    ///
    /// Uses --resume <session_id> to continue the conversation.
    pub async fn execute_follow_up(
        &self,
        session_id: &str,
        message: &str,
        output_tx: mpsc::Sender<OutputChunk>,
        event_tx: mpsc::Sender<RunEvent>,
        run_id: RunId,
        task_id: TaskId,
    ) -> Result<ExecutionResult, ExecutorError> {
        info!(
            session_id = %session_id,
            message_len = message.len(),
            "Starting session continuation"
        );

        // Emit ExecutionStarted event
        if event_tx
            .send(RunEvent::execution_started(run_id.clone(), task_id.clone()))
            .await
            .is_err()
        {
            warn!("Failed to send ExecutionStarted event");
        }

        // Create SDK executor with bypass permissions
        let mut sdk_executor = ClaudeExecutor::new(&self.config.claude_path)
            .with_permission_mode(PermissionMode::BypassPermissions);

        // Apply tool permissions from config
        if let Some(ref allowed) = self.config.allowed_tools {
            sdk_executor = sdk_executor.with_allowed_tools(allowed.clone());
        }
        if let Some(ref denied) = self.config.denied_tools {
            sdk_executor = sdk_executor.with_disallowed_tools(denied.clone());
        }

        // Create streaming handler
        let handler = Arc::new(StreamingHandler::new(
            output_tx.clone(),
            event_tx,
            run_id,
            task_id,
        ));

        // Execute follow-up via SDK
        let working_dir = Path::new(&self.config.working_dir);
        let result = sdk_executor
            .execute_follow_up(working_dir, message, session_id, handler.clone())
            .await
            .map_err(|e| ExecutorError::SdkError(e.to_string()))?;

        // Send final marker
        let _ = output_tx
            .send(OutputChunk {
                content: String::new(),
                is_final: true,
            })
            .await;

        let new_session_id = handler.session_id();
        let model_used = handler.model_used().unwrap_or(result.model_used);
        info!(
            session_id = ?new_session_id,
            model = %model_used,
            "Session continuation completed"
        );

        Ok(ExecutionResult {
            model_used,
            provider: "anthropic".to_string(),
            session_id: new_session_id,
        })
    }

    /// Build the prompt for a given agent and input.
    fn build_prompt(&self, agent_name: &str, input_json: &str) -> Result<String, ExecutorError> {
        match agent_name {
            "general" => Ok(self.build_general_prompt(input_json)),
            "support_triage" => Ok(self.build_support_triage_prompt(input_json)),
            _ => Err(ExecutorError::UnknownAgent(agent_name.to_string())),
        }
    }

    /// Build the general agent prompt - just passes the task directly.
    fn build_general_prompt(&self, input_json: &str) -> String {
        // Try to parse as JSON to extract "task" field, otherwise use as-is
        if let Ok(parsed) = serde_json::from_str::<Value>(input_json) {
            if let Some(task) = parsed.get("task").and_then(|t| t.as_str()) {
                return task.to_string();
            }
        }
        // If not JSON or no "task" field, use input directly
        input_json.to_string()
    }

    /// Build the support triage agent prompt.
    fn build_support_triage_prompt(&self, input_json: &str) -> String {
        format!(
            r#"You are a support ticket triage assistant. Analyze the following support ticket and provide a classification.

## Support Ticket
{}

## Instructions
Analyze the ticket and provide:
1. **Priority**: critical | high | medium | low
2. **Category**: billing | technical | account | feature_request | other
3. **Summary**: One sentence summary of the issue
4. **Suggested Action**: Brief recommended next step

## Response Format
Respond with a JSON object only, no additional text:
{{"priority": "...", "category": "...", "summary": "...", "suggested_action": "..."}}"#,
            input_json
        )
    }
}

/// Result of a successful execution.
#[derive(Debug)]
#[allow(dead_code)] // session_id is for API completeness
pub struct ExecutionResult {
    /// The model that was used for execution.
    pub model_used: String,
    /// The provider (e.g., "anthropic").
    pub provider: String,
    /// The session ID for continuation (if available).
    pub session_id: Option<String>,
}
