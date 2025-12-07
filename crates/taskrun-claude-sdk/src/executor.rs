//! Claude Code executor for running agents via subprocess.
//!
//! This module provides the main `ClaudeExecutor` type for executing
//! Claude Code agents using one-shot mode with streaming JSON output.

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

use crate::error::SdkError;
use crate::protocol::ControlHandler;
use crate::types::PermissionMode;

/// Result of a Claude Code execution.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// The session ID from Claude Code.
    pub session_id: String,

    /// The model that was used for execution.
    pub model_used: String,

    /// Duration of execution in milliseconds.
    pub duration_ms: u64,

    /// Whether the execution resulted in an error.
    pub is_error: bool,

    /// Error message if `is_error` is true.
    pub error_message: Option<String>,
}

/// Executor for Claude Code agents.
///
/// # Example
///
/// ```rust,no_run
/// use std::path::Path;
/// use std::sync::Arc;
/// use taskrun_claude_sdk::{ClaudeExecutor, AutoApproveHandler, PermissionMode};
///
/// async fn run() -> Result<(), Box<dyn std::error::Error>> {
///     let executor = ClaudeExecutor::new("claude")
///         .with_permission_mode(PermissionMode::BypassPermissions);
///
///     let (handler, mut rx) = AutoApproveHandler::new();
///
///     let result = executor.execute(
///         Path::new("."),
///         "What is 2 + 2?",
///         Arc::new(handler),
///     ).await?;
///
///     println!("Session: {}", result.session_id);
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ClaudeExecutor {
    /// Path to the Claude CLI executable.
    claude_path: String,

    /// Permission mode to use.
    permission_mode: PermissionMode,

    /// Model to use (optional).
    model: Option<String>,

    /// Maximum thinking tokens (optional).
    max_thinking_tokens: Option<u32>,

    /// System prompt (optional).
    system_prompt: Option<String>,

    /// Additional environment variables.
    env_vars: Vec<(String, String)>,
}

impl ClaudeExecutor {
    /// Create a new executor with the given path to the Claude CLI.
    ///
    /// The path can be just "claude" to use PATH lookup, or a full path.
    pub fn new(claude_path: impl Into<String>) -> Self {
        Self {
            claude_path: claude_path.into(),
            permission_mode: PermissionMode::Default,
            model: None,
            max_thinking_tokens: None,
            system_prompt: None,
            env_vars: Vec::new(),
        }
    }

    /// Set the permission mode.
    pub fn with_permission_mode(mut self, mode: PermissionMode) -> Self {
        self.permission_mode = mode;
        self
    }

    /// Set the model to use.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set maximum thinking tokens.
    pub fn with_max_thinking_tokens(mut self, tokens: u32) -> Self {
        self.max_thinking_tokens = Some(tokens);
        self
    }

    /// Set a system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Add an environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.push((key.into(), value.into()));
        self
    }

    /// Execute a prompt with Claude Code.
    ///
    /// This spawns a new Claude process and runs the given prompt.
    /// The handler receives all messages and responds to permission requests.
    pub async fn execute(
        &self,
        working_dir: &Path,
        prompt: &str,
        handler: Arc<dyn ControlHandler>,
    ) -> Result<ExecutionResult, SdkError> {
        self.execute_internal(working_dir, prompt, None, handler)
            .await
    }

    /// Execute a follow-up prompt in an existing session.
    ///
    /// This resumes a previous Claude session by its session ID.
    pub async fn execute_follow_up(
        &self,
        working_dir: &Path,
        prompt: &str,
        session_id: &str,
        handler: Arc<dyn ControlHandler>,
    ) -> Result<ExecutionResult, SdkError> {
        self.execute_internal(working_dir, prompt, Some(session_id), handler)
            .await
    }

    /// Internal execution implementation.
    async fn execute_internal(
        &self,
        working_dir: &Path,
        prompt: &str,
        session_id: Option<&str>,
        handler: Arc<dyn ControlHandler>,
    ) -> Result<ExecutionResult, SdkError> {
        info!(
            claude_path = %self.claude_path,
            working_dir = %working_dir.display(),
            prompt_len = prompt.len(),
            "Preparing Claude execution"
        );

        let mut cmd = Command::new(&self.claude_path);

        // Base arguments for one-shot execution with JSON output
        // Note: --input-format=stream-json enables control protocol which requires
        // different handling. For simple execution, just use output format.
        cmd.arg("--output-format=stream-json");

        // Add optional arguments
        if let Some(model) = &self.model {
            cmd.arg("--model").arg(model);
            info!(model = %model, "Using specified model");
        }

        if let Some(tokens) = self.max_thinking_tokens {
            cmd.arg("--max-thinking-tokens")
                .arg(tokens.to_string());
        }

        if let Some(system) = &self.system_prompt {
            cmd.arg("--system-prompt").arg(system);
        }

        // Session continuation
        if let Some(sid) = session_id {
            cmd.arg("--continue").arg(sid);
            info!(session_id = %sid, "Continuing session");
        }

        // The prompt itself
        cmd.arg("--print").arg(prompt);

        // Configure stdio - no stdin needed for one-shot mode
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(working_dir);

        // Add environment variables
        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

        info!("Spawning Claude process with args: --output-format=stream-json --print <prompt>");
        debug!("Full command: {:?}", cmd);

        let mut child = cmd.spawn().map_err(|e| {
            error!(error = %e, "Failed to spawn Claude process");
            e
        })?;

        info!("Claude process spawned successfully");

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SdkError::ProtocolError("Failed to get stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| SdkError::ProtocolError("Failed to get stderr".to_string()))?;

        info!("Got stdout/stderr handles");

        // Spawn stderr reader for logging
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            warn!(stderr = %trimmed, "Claude stderr");
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Error reading Claude stderr");
                        break;
                    }
                }
            }
        });

        // Spawn stdout reader that forwards messages to handler
        info!("Starting stdout reader for JSON messages");
        let handler_clone = handler.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let mut message_count = 0u64;

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        info!(total_messages = message_count, "Claude stdout closed (EOF)");
                        break;
                    }
                    Ok(bytes) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        message_count += 1;
                        info!(message_num = message_count, bytes = bytes, "Received message from Claude");

                        match serde_json::from_str::<crate::types::ClaudeMessage>(trimmed) {
                            Ok(message) => {
                                if let Err(e) = handler_clone.on_message(message).await {
                                    warn!(error = %e, "Handler error processing message");
                                }
                            }
                            Err(e) => {
                                warn!(error = %e, "Failed to parse Claude message");
                                // Log first 200 chars for debugging
                                let preview: String = trimmed.chars().take(200).collect();
                                warn!(preview = %preview, "Message preview");
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Error reading Claude stdout");
                        break;
                    }
                }
            }
        });

        info!("Waiting for Claude process to complete...");

        // Wait for process to complete
        let status = child.wait().await?;

        let exit_code = status.code().unwrap_or(-1);
        info!(exit_code = exit_code, success = status.success(), "Claude process exited");

        if !status.success() {
            return Err(SdkError::ProcessError(format!(
                "Claude exited with code {}",
                exit_code
            )));
        }

        // Note: In a real implementation, we'd capture session_id and model from
        // the messages received by the handler. For now, return placeholder values.
        Ok(ExecutionResult {
            session_id: "unknown".to_string(),
            model_used: self.model.clone().unwrap_or_else(|| "default".to_string()),
            duration_ms: 0,
            is_error: false,
            error_message: None,
        })
    }
}

/// Builder for creating ClaudeExecutor with additional configuration.
impl Default for ClaudeExecutor {
    fn default() -> Self {
        Self::new("claude")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_builder() {
        let executor = ClaudeExecutor::new("claude")
            .with_model("claude-sonnet-4-20250514")
            .with_permission_mode(PermissionMode::BypassPermissions)
            .with_max_thinking_tokens(10000)
            .with_system_prompt("You are a helpful assistant.")
            .with_env("ANTHROPIC_API_KEY", "test-key");

        assert_eq!(executor.claude_path, "claude");
        assert_eq!(executor.model, Some("claude-sonnet-4-20250514".to_string()));
        assert_eq!(executor.permission_mode, PermissionMode::BypassPermissions);
        assert_eq!(executor.max_thinking_tokens, Some(10000));
        assert_eq!(
            executor.system_prompt,
            Some("You are a helpful assistant.".to_string())
        );
        assert_eq!(executor.env_vars.len(), 1);
    }

    #[test]
    fn test_default_executor() {
        let executor = ClaudeExecutor::default();
        assert_eq!(executor.claude_path, "claude");
        assert_eq!(executor.permission_mode, PermissionMode::Default);
        assert!(executor.model.is_none());
    }
}
