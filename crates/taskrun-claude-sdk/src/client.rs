//! Client implementations for the Claude Code SDK.
//!
//! This module provides ready-to-use implementations of the `ControlHandler` trait.

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::trace;

use crate::error::SdkError;
use crate::protocol::ControlHandler;
use crate::types::{ClaudeMessage, PermissionResult};

/// A handler that automatically approves all tool uses.
///
/// This is useful for automated execution where you want Claude to run
/// without any permission prompts. Use with caution - this allows Claude
/// to execute any tool without confirmation.
///
/// # Example
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use taskrun_claude_sdk::AutoApproveHandler;
///
/// let (handler, mut rx) = AutoApproveHandler::new();
///
/// // Spawn a task to process messages
/// tokio::spawn(async move {
///     while let Some(msg) = rx.recv().await {
///         println!("Received: {:?}", msg);
///     }
/// });
///
/// // Use Arc::new(handler) with ClaudeExecutor
/// ```
pub struct AutoApproveHandler {
    message_tx: mpsc::UnboundedSender<ClaudeMessage>,
}

impl AutoApproveHandler {
    /// Create a new auto-approve handler with a message receiver.
    ///
    /// Returns the handler and a receiver for streaming messages.
    pub fn new() -> (Self, mpsc::UnboundedReceiver<ClaudeMessage>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { message_tx: tx }, rx)
    }

    /// Create a handler with a bounded channel.
    ///
    /// This is useful when you want backpressure on message processing.
    pub fn with_capacity(
        capacity: usize,
    ) -> (BoundedAutoApproveHandler, mpsc::Receiver<ClaudeMessage>) {
        let (tx, rx) = mpsc::channel(capacity);
        (BoundedAutoApproveHandler { message_tx: tx }, rx)
    }
}

#[async_trait]
impl ControlHandler for AutoApproveHandler {
    async fn on_can_use_tool(
        &self,
        tool_name: String,
        input: Value,
    ) -> Result<PermissionResult, SdkError> {
        trace!("Auto-approving tool: {}", tool_name);
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
        trace!("Auto-approving hook callback: {}", callback_id);
        Ok(json!({
            "hookSpecificOutput": {
                "permissionDecision": "allow"
            }
        }))
    }

    async fn on_message(&self, message: ClaudeMessage) -> Result<(), SdkError> {
        // Send to channel, ignoring errors (receiver might be dropped)
        self.message_tx.send(message).ok();
        Ok(())
    }
}

/// A bounded variant of AutoApproveHandler.
///
/// Uses a bounded channel which provides backpressure when the receiver
/// is slow to process messages.
pub struct BoundedAutoApproveHandler {
    message_tx: mpsc::Sender<ClaudeMessage>,
}

#[async_trait]
impl ControlHandler for BoundedAutoApproveHandler {
    async fn on_can_use_tool(
        &self,
        tool_name: String,
        input: Value,
    ) -> Result<PermissionResult, SdkError> {
        trace!("Auto-approving tool: {}", tool_name);
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
        trace!("Auto-approving hook callback: {}", callback_id);
        Ok(json!({
            "hookSpecificOutput": {
                "permissionDecision": "allow"
            }
        }))
    }

    async fn on_message(&self, message: ClaudeMessage) -> Result<(), SdkError> {
        // Try to send, return error if channel is closed
        self.message_tx
            .send(message)
            .await
            .map_err(|_| SdkError::ChannelClosed)?;
        Ok(())
    }
}

/// A handler that denies all tool uses.
///
/// Useful for testing or when you want to see what tools Claude
/// would try to use without actually allowing execution.
pub struct DenyAllHandler {
    message_tx: mpsc::UnboundedSender<ClaudeMessage>,
    deny_message: String,
}

impl DenyAllHandler {
    /// Create a new deny-all handler.
    pub fn new(deny_message: impl Into<String>) -> (Self, mpsc::UnboundedReceiver<ClaudeMessage>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                message_tx: tx,
                deny_message: deny_message.into(),
            },
            rx,
        )
    }
}

#[async_trait]
impl ControlHandler for DenyAllHandler {
    async fn on_can_use_tool(
        &self,
        tool_name: String,
        _input: Value,
    ) -> Result<PermissionResult, SdkError> {
        trace!("Denying tool: {}", tool_name);
        Ok(PermissionResult::Deny {
            message: self.deny_message.clone(),
            interrupt: Some(false),
        })
    }

    async fn on_hook_callback(
        &self,
        callback_id: String,
        _input: Value,
        _tool_use_id: Option<String>,
    ) -> Result<Value, SdkError> {
        trace!("Denying hook callback: {}", callback_id);
        Ok(json!({
            "hookSpecificOutput": {
                "permissionDecision": "deny",
                "message": self.deny_message
            }
        }))
    }

    async fn on_message(&self, message: ClaudeMessage) -> Result<(), SdkError> {
        self.message_tx.send(message).ok();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_auto_approve_handler() {
        let (handler, mut rx) = AutoApproveHandler::new();

        // Test tool approval
        let result = handler
            .on_can_use_tool("Bash".to_string(), json!({"command": "ls"}))
            .await
            .unwrap();

        match result {
            PermissionResult::Allow { updated_input, .. } => {
                assert_eq!(updated_input["command"], "ls");
            }
            _ => panic!("Expected Allow"),
        }

        // Test message forwarding
        let msg = ClaudeMessage::System {
            session_id: Some("test".to_string()),
            subtype: None,
            model: None,
            cwd: None,
        };
        handler.on_message(msg).await.unwrap();

        let received = rx.try_recv().unwrap();
        assert!(matches!(received, ClaudeMessage::System { .. }));
    }

    #[tokio::test]
    async fn test_deny_all_handler() {
        let (handler, _rx) = DenyAllHandler::new("Not allowed in test mode");

        let result = handler
            .on_can_use_tool("Bash".to_string(), json!({"command": "rm -rf /"}))
            .await
            .unwrap();

        match result {
            PermissionResult::Deny { message, .. } => {
                assert_eq!(message, "Not allowed in test mode");
            }
            _ => panic!("Expected Deny"),
        }
    }
}
