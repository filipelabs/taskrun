//! Agent execution via Claude Code SDK.
//!
//! This module uses the `taskrun-claude-sdk` crate for structured communication
//! with Claude Code, providing streaming output and session tracking.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};
use taskrun_claude_sdk::{
    ClaudeExecutor, ClaudeMessage, ContentDelta, ContentItem, ControlHandler, PermissionMode,
    PermissionResult, SdkError, StreamEvent,
};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, info, trace, warn};

/// Errors that can occur during agent execution.
#[derive(Debug, Error)]
pub enum ExecutorError {
    #[allow(dead_code)] // Kept for potential future use
    #[error("Claude CLI not found at '{0}'. Ensure Claude Code is installed.")]
    ClaudeNotFound(String),

    #[error("Failed to spawn Claude process: {0}")]
    SpawnError(#[from] std::io::Error),

    #[allow(dead_code)] // Kept for potential future use
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

/// Handler that streams Claude messages as output chunks.
struct StreamingHandler {
    output_tx: mpsc::Sender<OutputChunk>,
    session_id: Arc<Mutex<Option<String>>>,
}

impl StreamingHandler {
    fn new(output_tx: mpsc::Sender<OutputChunk>) -> Self {
        Self {
            output_tx,
            session_id: Arc::new(Mutex::new(None)),
        }
    }

    fn session_id(&self) -> Option<String> {
        self.session_id.lock().unwrap().clone()
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
        info!(message_type = msg_type, "StreamingHandler received message");

        match message {
            ClaudeMessage::System { session_id, model, .. } => {
                // Capture session ID for future use
                info!(session_id = ?session_id, model = ?model, "System message received");
                if let Some(sid) = session_id {
                    info!(session_id = %sid, "Captured session ID");
                    *self.session_id.lock().unwrap() = Some(sid);
                }
            }
            ClaudeMessage::Assistant { message, .. } => {
                // Extract text content and stream it
                info!(content_count = message.content.len(), "Assistant message received");
                for content in message.content {
                    if let ContentItem::Text { text } = content {
                        info!(text_len = text.len(), "Streaming assistant text chunk");
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
                if let StreamEvent::ContentBlockDelta { delta, index } = event {
                    if let ContentDelta::TextDelta { text } = delta {
                        info!(text_len = text.len(), block_index = index, "Streaming text delta");
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
            ClaudeMessage::Result {
                is_error,
                duration_ms,
                ..
            } => {
                info!(
                    is_error = ?is_error,
                    duration_ms = ?duration_ms,
                    "Execution result received"
                );
            }
            ClaudeMessage::ToolUse { tool_name, .. } => {
                info!(tool = %tool_name, "Tool use message");
            }
            ClaudeMessage::ToolResult { is_error, .. } => {
                info!(is_error = ?is_error, "Tool result message");
            }
            ClaudeMessage::Unknown(ref value) => {
                // Log the full unknown message for debugging
                let full_json = serde_json::to_string(value)
                    .unwrap_or_else(|_| "failed to serialize".to_string());
                // Check if this is a control_response (expected)
                if value.get("type").and_then(|t| t.as_str()) == Some("control_response") {
                    info!(len = full_json.len(), "Received control_response (expected)");
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
    /// Path to the claude CLI binary.
    claude_path: String,
}

impl ClaudeCodeExecutor {
    /// Create a new executor with the given claude CLI path.
    pub fn new(claude_path: String) -> Self {
        Self { claude_path }
    }

    /// Execute an agent with the given input, streaming output via the channel.
    ///
    /// Returns when execution completes (successfully or with error).
    pub async fn execute(
        &self,
        agent_name: &str,
        input_json: &str,
        output_tx: mpsc::Sender<OutputChunk>,
    ) -> Result<ExecutionResult, ExecutorError> {
        info!(
            agent = %agent_name,
            claude_path = %self.claude_path,
            input_len = input_json.len(),
            "Starting agent execution"
        );

        // Build the prompt based on agent type
        let prompt = self.build_prompt(agent_name, input_json)?;
        info!(prompt_len = prompt.len(), "Built prompt for agent");

        info!(agent = %agent_name, "Creating Claude Code SDK executor");
        debug!(prompt = %prompt, "Full prompt");

        // Create SDK executor with bypass permissions (auto-approve all)
        let sdk_executor = ClaudeExecutor::new(&self.claude_path)
            .with_permission_mode(PermissionMode::BypassPermissions);

        // Create streaming handler
        let handler = Arc::new(StreamingHandler::new(output_tx.clone()));

        // Execute via SDK
        let result = sdk_executor
            .execute(Path::new("."), &prompt, handler.clone())
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
        info!(
            session_id = ?session_id,
            model = %result.model_used,
            "Claude Code execution completed"
        );

        Ok(ExecutionResult {
            model_used: result.model_used,
            provider: "anthropic".to_string(),
            session_id,
        })
    }

    /// Build the prompt for a given agent and input.
    fn build_prompt(&self, agent_name: &str, input_json: &str) -> Result<String, ExecutorError> {
        match agent_name {
            "support_triage" => Ok(self.build_support_triage_prompt(input_json)),
            _ => Err(ExecutorError::UnknownAgent(agent_name.to_string())),
        }
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
pub struct ExecutionResult {
    /// The model that was used for execution.
    pub model_used: String,
    /// The provider (e.g., "anthropic").
    pub provider: String,
    /// The session ID for continuation (if available).
    #[allow(dead_code)] // Exposed for future session continuation support
    pub session_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_support_triage_prompt() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let input = r#"{"ticket_id": "123", "subject": "Cannot login"}"#;
        let prompt = executor.build_support_triage_prompt(input);

        assert!(prompt.contains("support ticket triage"));
        assert!(prompt.contains("ticket_id"));
        assert!(prompt.contains("priority"));
    }

    #[test]
    fn test_unknown_agent() {
        let executor = ClaudeCodeExecutor::new("claude".to_string());
        let result = executor.build_prompt("unknown_agent", "{}");
        assert!(matches!(result, Err(ExecutorError::UnknownAgent(_))));
    }
}
