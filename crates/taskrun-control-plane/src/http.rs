//! HTTP server for worker enrollment.
//!
//! This provides the `/v1/enroll` endpoint that allows workers
//! to obtain certificates via bootstrap token + CSR flow.

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::crypto::hash_token;
use crate::state::AppState;

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

/// Error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Create the HTTP router for enrollment.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/enroll", post(enroll))
        .route("/health", get(health_check))
        .with_state(state)
}

/// Health check endpoint.
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Worker enrollment endpoint.
///
/// Validates bootstrap token, signs CSR, and returns worker certificate.
async fn enroll(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EnrollRequest>,
) -> impl IntoResponse {
    // Check if CA is configured
    let ca = match &state.ca {
        Some(ca) => ca,
        None => {
            error!("Enrollment requested but CA is not configured");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "Certificate authority not configured".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Validate bootstrap token
    let token_hash = hash_token(&req.bootstrap_token);

    let token_valid = {
        let mut tokens = state.bootstrap_tokens.write().await;
        if let Some(token) = tokens.get_mut(&token_hash) {
            if token.is_valid() {
                token.consume();
                info!(token_hash = %token_hash, "Bootstrap token consumed");
                true
            } else {
                warn!(token_hash = %token_hash, consumed = token.consumed, "Invalid or expired token");
                false
            }
        } else {
            warn!("Unknown bootstrap token attempted");
            false
        }
    };

    if !token_valid {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid or expired bootstrap token".to_string(),
            }),
        )
            .into_response();
    }

    // Sign the CSR
    match ca.sign_csr(&req.csr) {
        Ok(signed) => {
            info!(
                worker_id = %signed.worker_id,
                expires_at = %signed.expires_at,
                "Worker certificate issued"
            );

            (
                StatusCode::OK,
                Json(EnrollResponse {
                    worker_cert: signed.cert_pem,
                    ca_cert: ca.ca_cert_pem().to_string(),
                    expires_at: signed.expires_at.to_rfc3339(),
                }),
            )
                .into_response()
        }
        Err(e) => {
            error!(error = %e, "Failed to sign CSR");
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Failed to sign CSR: {}", e),
                }),
            )
                .into_response()
        }
    }
}
