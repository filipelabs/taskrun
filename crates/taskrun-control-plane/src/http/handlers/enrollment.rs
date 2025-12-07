//! Worker enrollment handler.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use tracing::{error, info, warn};

use crate::crypto::hash_token;
use crate::http::responses::{EnrollRequest, EnrollResponse, ErrorResponse};
use crate::state::AppState;

/// Worker enrollment endpoint.
///
/// Validates bootstrap token, signs CSR, and returns worker certificate.
pub async fn enroll(
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
