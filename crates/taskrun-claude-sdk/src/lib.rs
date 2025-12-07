//! Claude Code SDK for TaskRun
//!
//! This crate provides a clean abstraction for executing Claude Code agents
//! via subprocess with the control protocol for bidirectional communication.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::path::Path;
//! use std::sync::Arc;
//! use taskrun_claude_sdk::{ClaudeExecutor, AutoApproveHandler, PermissionMode};
//!
//! async fn run_agent() -> Result<(), Box<dyn std::error::Error>> {
//!     let executor = ClaudeExecutor::new("claude")
//!         .with_permission_mode(PermissionMode::BypassPermissions);
//!
//!     let (handler, mut rx) = AutoApproveHandler::new();
//!
//!     let result = executor.execute(
//!         Path::new("."),
//!         "What is 2 + 2?",
//!         Arc::new(handler),
//!     ).await?;
//!
//!     println!("Session ID: {}", result.session_id);
//!     Ok(())
//! }
//! ```

mod client;
mod error;
mod executor;
mod protocol;
mod types;

// Re-export main types
pub use client::{AutoApproveHandler, BoundedAutoApproveHandler, DenyAllHandler};
pub use error::SdkError;
pub use executor::{ClaudeExecutor, ExecutionResult};
pub use protocol::ControlHandler;
pub use types::{
    AssistantMessage, ClaudeMessage, ContentDelta, ContentItem, ControlRequest, ControlResponse,
    MessageDelta, PermissionMode, PermissionResult, PermissionUpdate, PermissionUpdateDestination,
    PermissionUpdateType, SdkControlRequest, SdkControlRequestType, StreamEvent, ToolData,
    UserMessage,
};
