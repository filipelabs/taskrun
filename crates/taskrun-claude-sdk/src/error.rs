//! Error types for the Claude Code SDK.

use thiserror::Error;

/// Errors that can occur during Claude Code SDK operations.
#[derive(Debug, Error)]
pub enum SdkError {
    /// Claude CLI executable not found.
    #[error("Claude CLI not found at '{0}'. Ensure Claude Code is installed.")]
    ClaudeNotFound(String),

    /// Failed to spawn the Claude process.
    #[error("Failed to spawn Claude process: {0}")]
    SpawnError(#[from] std::io::Error),

    /// Claude process exited with an error.
    #[error("Claude process exited with error: {0}")]
    ProcessError(String),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Protocol error during communication.
    #[error("Protocol error: {0}")]
    ProtocolError(String),

    /// Channel send error.
    #[error("Channel closed")]
    ChannelClosed,

    /// Timeout waiting for response.
    #[error("Timeout waiting for response")]
    Timeout,
}
