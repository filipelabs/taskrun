//! Agent execution via Claude Code subprocess.

use std::process::Stdio;

use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Errors that can occur during agent execution.
#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("Claude CLI not found at '{0}'. Ensure Claude Code is installed.")]
    ClaudeNotFound(String),

    #[error("Failed to spawn Claude process: {0}")]
    SpawnError(#[from] std::io::Error),

    #[error("Claude process exited with error: {0}")]
    ProcessError(String),

    #[error("Unknown agent: {0}")]
    UnknownAgent(String),

}

/// Output chunk from Claude Code execution.
#[derive(Debug, Clone)]
pub struct OutputChunk {
    pub content: String,
    pub is_final: bool,
}

/// Executes agents via Claude Code CLI subprocess.
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
        // Build the prompt based on agent type
        let prompt = self.build_prompt(agent_name, input_json)?;

        info!(agent = %agent_name, "Starting Claude Code execution");
        debug!(prompt = %prompt, "Full prompt");

        // Spawn claude subprocess
        let mut cmd = Command::new(&self.claude_path);
        cmd.arg("--print") // Non-interactive mode, print to stdout
            .arg("--output-format")
            .arg("text") // Plain text output
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ExecutorError::ClaudeNotFound(self.claude_path.clone())
            } else {
                ExecutorError::SpawnError(e)
            }
        })?;

        // Write prompt to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        // Stream stdout line by line
        let stdout = child.stdout.take().expect("stdout should be captured");
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        let mut line_count = 0;
        while let Ok(Some(line)) = lines.next_line().await {
            line_count += 1;
            debug!(line = %line, seq = line_count, "Output line");

            if output_tx
                .send(OutputChunk {
                    content: line,
                    is_final: false,
                })
                .await
                .is_err()
            {
                warn!("Output channel closed, stopping execution");
                break;
            }
        }

        // Wait for process to complete
        let status = child.wait().await?;

        // Capture stderr if process failed
        if !status.success() {
            let stderr = child.stderr.take();
            let error_msg = if let Some(stderr) = stderr {
                let mut reader = BufReader::new(stderr);
                let mut error = String::new();
                let _ = tokio::io::AsyncReadExt::read_to_string(&mut reader, &mut error).await;
                error
            } else {
                format!("Process exited with code: {:?}", status.code())
            };

            error!(error = %error_msg, "Claude process failed");
            return Err(ExecutorError::ProcessError(error_msg));
        }

        // Send final marker
        let _ = output_tx
            .send(OutputChunk {
                content: String::new(),
                is_final: true,
            })
            .await;

        info!(lines = line_count, "Claude Code execution completed");

        Ok(ExecutionResult {
            model_used: "claude-sonnet-4-20250514".to_string(), // Default model
            provider: "anthropic".to_string(),
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
