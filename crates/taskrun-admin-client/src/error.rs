//! Error types for the admin client.

use thiserror::Error;

/// Errors that can occur when using the admin client.
#[derive(Debug, Error)]
pub enum ClientError {
    /// Failed to establish connection.
    #[error("connection failed: {0}")]
    Connection(String),

    /// gRPC error from the server.
    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),

    /// HTTP error.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Resource not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(String),
}
