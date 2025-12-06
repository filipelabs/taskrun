//! Generated gRPC code and converters for TaskRun.
//!
//! This crate contains:
//! - Generated protobuf message types
//! - Generated gRPC service stubs (client and server)
//! - Converters between proto types and domain types

pub mod convert;

/// Generated protobuf types and services.
pub mod pb {
    // Include the generated code
    // The path matches the proto package: taskrun.v1
    include!("gen/taskrun.v1.rs");
}

// Re-export commonly used types
pub use pb::run_service_client::RunServiceClient;
pub use pb::run_service_server::{RunService, RunServiceServer};
pub use pb::task_service_client::TaskServiceClient;
pub use pb::task_service_server::{TaskService, TaskServiceServer};
