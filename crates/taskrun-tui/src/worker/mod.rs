//! Worker TUI module.
//!
//! This module provides an interactive terminal UI for running a TaskRun worker.

mod app;
mod backend;
mod connection;
mod event;
mod executor;
mod render;
mod setup;
mod state;

pub use app::run_worker_tui;
pub use state::WorkerConfig;
