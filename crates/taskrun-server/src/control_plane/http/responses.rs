//! HTTP request and response types.

use serde::{Deserialize, Serialize};

// ============================================================================
// Enrollment types
// ============================================================================

/// Request body for the enroll endpoint.
#[derive(Debug, Deserialize)]
pub struct EnrollRequest {
    /// Bootstrap token (base64 encoded).
    pub bootstrap_token: String,

    /// Certificate Signing Request (PEM encoded).
    pub csr: String,
}

/// Response body for the enroll endpoint.
#[derive(Debug, Serialize)]
pub struct EnrollResponse {
    /// Signed worker certificate (PEM encoded).
    pub worker_cert: String,

    /// CA certificate (PEM encoded).
    pub ca_cert: String,

    /// Certificate expiration time (ISO 8601).
    pub expires_at: String,
}

// ============================================================================
// Error types
// ============================================================================

/// Error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ============================================================================
// Worker list types
// ============================================================================

/// Response for a single worker.
#[derive(Debug, Serialize)]
pub struct WorkerResponse {
    pub worker_id: String,
    pub hostname: String,
    pub version: String,
    pub status: String,
    pub active_runs: u32,
    pub max_concurrent_runs: u32,
    pub last_heartbeat: String,
    pub agents: Vec<AgentResponse>,
}

/// Agent information in worker response.
#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub name: String,
    pub description: String,
    pub backends: Vec<BackendResponse>,
}

/// Model backend information.
#[derive(Debug, Serialize)]
pub struct BackendResponse {
    pub provider: String,
    pub model_name: String,
}
