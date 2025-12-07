//! Control protocol handling for Claude Code communication.
//!
//! This module implements the bidirectional JSON protocol for communicating
//! with the Claude Code CLI over stdin/stdout.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use tracing::{debug, error, info, trace, warn};

use crate::error::SdkError;
use crate::types::{
    ClaudeMessage, ControlRequest, ControlResponse, ControlResponseType, PermissionMode,
    PermissionResult, SdkControlRequest, SdkControlRequestType,
};

/// Handler trait for control protocol callbacks.
///
/// Implement this trait to customize how your application responds to
/// tool use permission requests and hook callbacks from Claude Code.
#[async_trait]
pub trait ControlHandler: Send + Sync {
    /// Called when Claude wants to use a tool.
    ///
    /// Return `PermissionResult::Allow` to allow the tool use,
    /// or `PermissionResult::Deny` to block it.
    async fn on_can_use_tool(
        &self,
        tool_name: String,
        input: Value,
    ) -> Result<PermissionResult, SdkError>;

    /// Called for hook callbacks from Claude Code.
    async fn on_hook_callback(
        &self,
        callback_id: String,
        input: Value,
        tool_use_id: Option<String>,
    ) -> Result<Value, SdkError>;

    /// Called when a message is received from Claude.
    ///
    /// This is called for all message types (assistant, tool use, etc.)
    /// and can be used to stream output or track progress.
    async fn on_message(&self, message: ClaudeMessage) -> Result<(), SdkError>;
}

/// Protocol peer for bidirectional communication with Claude Code.
///
/// Manages the stdin/stdout streams and handles the control protocol
/// message exchange.
pub struct ProtocolPeer {
    stdin: Arc<Mutex<ChildStdin>>,
    initialized: Arc<Mutex<bool>>,
}

impl ProtocolPeer {
    /// Spawn a new protocol peer and start the read loop.
    ///
    /// This takes ownership of the stdin/stdout streams from the Claude process
    /// and spawns a background task to handle incoming messages.
    ///
    /// Returns the peer for sending messages.
    pub fn spawn(
        stdin: ChildStdin,
        stdout: ChildStdout,
        handler: Arc<dyn ControlHandler>,
    ) -> Self {
        info!("ProtocolPeer::spawn - starting read loop");
        let stdin = Arc::new(Mutex::new(stdin));
        let stdin_clone = Arc::clone(&stdin);

        // Spawn the read loop
        tokio::spawn(async move {
            info!("Protocol read loop task started");
            if let Err(e) = Self::read_loop(stdout, handler, stdin_clone).await {
                error!("Protocol read loop error: {}", e);
            }
            info!("Protocol read loop task ended");
        });

        Self {
            stdin,
            initialized: Arc::new(Mutex::new(false)),
        }
    }

    /// Initialize the control protocol.
    ///
    /// This should be called once after spawning before sending any other messages.
    pub async fn initialize(&self, hooks: Option<Value>) -> Result<(), SdkError> {
        let request = SdkControlRequest::new(SdkControlRequestType::Initialize { hooks });
        self.send_json(&request).await?;
        *self.initialized.lock().await = true;
        debug!("Control protocol initialized");
        Ok(())
    }

    /// Set the permission mode for Claude Code.
    pub async fn set_permission_mode(&self, mode: PermissionMode) -> Result<(), SdkError> {
        let request = SdkControlRequest::new(SdkControlRequestType::SetPermissionMode { mode });
        self.send_json(&request).await?;
        debug!("Permission mode set to: {}", mode);
        Ok(())
    }

    /// Send a JSON message to Claude's stdin.
    async fn send_json<T: serde::Serialize>(&self, message: &T) -> Result<(), SdkError> {
        let json = serde_json::to_string(message)?;
        info!(json_len = json.len(), "Sending control message to Claude stdin");
        trace!("Sending to stdin: {}", json);

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(json.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        info!("Control message sent and flushed");
        Ok(())
    }

    /// Main read loop for processing stdout messages.
    async fn read_loop(
        stdout: ChildStdout,
        handler: Arc<dyn ControlHandler>,
        stdin: Arc<Mutex<ChildStdin>>,
    ) -> Result<(), SdkError> {
        info!("Read loop started, waiting for Claude stdout...");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let mut message_count = 0u64;

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;

            if bytes_read == 0 {
                info!(total_messages = message_count, "Claude process stdout closed (EOF)");
                break;
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            message_count += 1;
            info!(
                message_num = message_count,
                bytes = bytes_read,
                "Received message from Claude stdout"
            );
            trace!("Raw stdout: {}", trimmed);

            match serde_json::from_str::<ClaudeMessage>(trimmed) {
                Ok(message) => {
                    // Log message type
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
                    info!(message_type = msg_type, "Parsed Claude message");

                    // Handle control requests specially
                    if let ClaudeMessage::ControlRequest {
                        request_id,
                        request,
                    } = &message
                    {
                        info!(request_id = %request_id, "Processing control request");
                        Self::handle_control_request(
                            request_id.clone(),
                            request.clone(),
                            &handler,
                            &stdin,
                        )
                        .await?;
                    } else {
                        // Forward other messages to the handler
                        if let Err(e) = handler.on_message(message).await {
                            warn!("Handler error processing message: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        raw_len = trimmed.len(),
                        "Failed to parse Claude message"
                    );
                    // Log first 200 chars of raw message for debugging
                    let preview: String = trimmed.chars().take(200).collect();
                    warn!(preview = %preview, "Message preview");
                }
            }
        }

        Ok(())
    }

    /// Handle a control request from Claude Code.
    async fn handle_control_request(
        request_id: String,
        request: ControlRequest,
        handler: &Arc<dyn ControlHandler>,
        stdin: &Arc<Mutex<ChildStdin>>,
    ) -> Result<(), SdkError> {
        debug!("Handling control request: {:?}", request);

        let response = match request {
            ControlRequest::CanUseTool {
                tool_name, input, ..
            } => {
                match handler.on_can_use_tool(tool_name, input).await {
                    Ok(result) => ControlResponse::new(ControlResponseType::Success {
                        request_id: request_id.clone(),
                        response: Some(serde_json::to_value(result)?),
                    }),
                    Err(e) => ControlResponse::new(ControlResponseType::Error {
                        request_id: request_id.clone(),
                        error: Some(e.to_string()),
                    }),
                }
            }
            ControlRequest::HookCallback {
                callback_id,
                input,
                tool_use_id,
            } => match handler
                .on_hook_callback(callback_id, input, tool_use_id)
                .await
            {
                Ok(result) => ControlResponse::new(ControlResponseType::Success {
                    request_id: request_id.clone(),
                    response: Some(result),
                }),
                Err(e) => ControlResponse::new(ControlResponseType::Error {
                    request_id: request_id.clone(),
                    error: Some(e.to_string()),
                }),
            },
        };

        // Send response back to Claude
        let json = serde_json::to_string(&response)?;
        trace!("Sending control response: {}", json);

        let mut stdin_guard = stdin.lock().await;
        stdin_guard.write_all(json.as_bytes()).await?;
        stdin_guard.write_all(b"\n").await?;
        stdin_guard.flush().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::sync::mpsc;

    #[allow(dead_code)] // Used in future integration tests
    struct TestHandler {
        #[allow(dead_code)]
        messages_tx: mpsc::UnboundedSender<ClaudeMessage>,
    }

    #[async_trait]
    impl ControlHandler for TestHandler {
        async fn on_can_use_tool(
            &self,
            _tool_name: String,
            input: Value,
        ) -> Result<PermissionResult, SdkError> {
            Ok(PermissionResult::Allow {
                updated_input: input,
                updated_permissions: None,
            })
        }

        async fn on_hook_callback(
            &self,
            _callback_id: String,
            _input: Value,
            _tool_use_id: Option<String>,
        ) -> Result<Value, SdkError> {
            Ok(json!({ "hookSpecificOutput": { "permissionDecision": "allow" } }))
        }

        async fn on_message(&self, message: ClaudeMessage) -> Result<(), SdkError> {
            self.messages_tx.send(message).ok();
            Ok(())
        }
    }

    #[test]
    fn test_control_response_serialization() {
        let response = ControlResponse::new(ControlResponseType::Success {
            request_id: "test-123".to_string(),
            response: Some(json!({ "allowed": true })),
        });

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("control_response"));
        assert!(json.contains("test-123"));
    }

    #[test]
    fn test_sdk_control_request_serialization() {
        let request =
            SdkControlRequest::new(SdkControlRequestType::SetPermissionMode {
                mode: PermissionMode::BypassPermissions,
            });

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("control_request"));
        assert!(json.contains("set_permission_mode"));
        assert!(json.contains("bypassPermissions"));
    }
}
