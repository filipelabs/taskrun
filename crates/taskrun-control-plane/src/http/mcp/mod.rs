//! MCP (Model Context Protocol) HTTP tools.
//!
//! Provides HTTP endpoints for MCP tools:
//! - `list_workers` - List connected workers
//! - `start_new_task` - Create and start a new task
//! - `read_task` - Get task status, output, events, and chat
//! - `continue_task` - Send a follow-up message

mod tools;
mod types;

pub use tools::{continue_task, list_workers, read_task, start_new_task};
