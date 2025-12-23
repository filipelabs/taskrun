//! Control Plane functionality.
//!
//! This module provides the core control plane functionality for TaskRun,
//! including gRPC services, scheduling, and state management.

pub mod config;
pub mod crypto;
pub mod http;
pub mod metrics;
pub mod scheduler;
pub mod service;
pub mod state;

pub use scheduler::Scheduler;
pub use service::{RunServiceImpl, TaskServiceImpl, WorkerServiceImpl};
