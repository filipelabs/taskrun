//! Admin client library for TaskRun TUI.
//!
//! Provides gRPC and HTTP clients for communicating with the TaskRun control plane.

pub mod error;
pub mod grpc;
pub mod http;

pub use error::ClientError;
pub use grpc::{AdminClient, TaskClient, WorkerClient};
pub use http::HttpClient;
