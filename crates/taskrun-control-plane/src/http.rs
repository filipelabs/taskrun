//! HTTP server for worker enrollment.
//!
//! This provides the `/v1/enroll` endpoint that allows workers
//! to obtain certificates via bootstrap token + CSR flow.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{header, StatusCode},
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

// ============================================================================
// Worker list API types
// ============================================================================

/// Response for a single worker.
#[derive(Debug, Serialize)]
struct WorkerResponse {
    worker_id: String,
    hostname: String,
    version: String,
    status: String,
    active_runs: u32,
    max_concurrent_runs: u32,
    last_heartbeat: String,
    agents: Vec<AgentResponse>,
}

/// Agent information in worker response.
#[derive(Debug, Serialize)]
struct AgentResponse {
    name: String,
    description: String,
    backends: Vec<BackendResponse>,
}

/// Model backend information.
#[derive(Debug, Serialize)]
struct BackendResponse {
    provider: String,
    model_name: String,
}

/// Create the HTTP router for enrollment.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/enroll", post(enroll))
        .route("/v1/workers", get(list_workers_json))
        .route("/ui/workers", get(list_workers_html))
        .route("/health", get(health_check))
        .route("/metrics", get(metrics_handler))
        .with_state(state)
}

/// Health check endpoint.
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Prometheus metrics endpoint.
async fn metrics_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let body = crate::metrics::collect_metrics(&state).await;
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        body,
    )
}

// ============================================================================
// Worker list endpoints
// ============================================================================

/// List workers as JSON.
async fn list_workers_json(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let workers = state.workers.read().await;
    let response: Vec<WorkerResponse> = workers
        .values()
        .map(|w| WorkerResponse {
            worker_id: w.info.worker_id.as_str().to_string(),
            hostname: w.info.hostname.clone(),
            version: w.info.version.clone(),
            status: format!("{:?}", w.status).to_uppercase(),
            active_runs: w.active_runs,
            max_concurrent_runs: w.max_concurrent_runs,
            last_heartbeat: w.last_heartbeat.to_rfc3339(),
            agents: w
                .info
                .agents
                .iter()
                .map(|a| AgentResponse {
                    name: a.name.clone(),
                    description: a.description.clone(),
                    backends: a
                        .backends
                        .iter()
                        .map(|b| BackendResponse {
                            provider: b.provider.clone(),
                            model_name: b.model_name.clone(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    Json(response)
}

/// List workers as HTML page.
async fn list_workers_html(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let workers = state.workers.read().await;
    let now = chrono::Utc::now();

    let mut rows = String::new();
    for worker in workers.values() {
        let status_color = match worker.status {
            taskrun_core::WorkerStatus::Idle => "#22c55e",
            taskrun_core::WorkerStatus::Busy => "#eab308",
            taskrun_core::WorkerStatus::Draining => "#f97316",
            taskrun_core::WorkerStatus::Error => "#ef4444",
        };

        let heartbeat_ago = {
            let duration = now.signed_duration_since(worker.last_heartbeat);
            if duration.num_seconds() < 60 {
                format!("{}s ago", duration.num_seconds())
            } else if duration.num_minutes() < 60 {
                format!("{}m ago", duration.num_minutes())
            } else {
                format!("{}h ago", duration.num_hours())
            }
        };

        let agents_html: Vec<String> = worker
            .info
            .agents
            .iter()
            .map(|a| {
                let models: Vec<String> = a
                    .backends
                    .iter()
                    .map(|b| format!("{}/{}", b.provider, b.model_name))
                    .collect();
                format!(
                    "<strong>{}</strong><br><small>{}</small>",
                    a.name,
                    models.join(", ")
                )
            })
            .collect();

        rows.push_str(&format!(
            r#"<tr>
                <td>{}</td>
                <td>{}</td>
                <td>{}</td>
                <td><span style="color: {}; font-weight: bold;">{:?}</span></td>
                <td>{}/{}</td>
                <td>{}</td>
                <td>{}</td>
            </tr>"#,
            worker.info.worker_id.as_str(),
            worker.info.hostname,
            worker.info.version,
            status_color,
            worker.status,
            worker.active_runs,
            worker.max_concurrent_runs,
            heartbeat_ago,
            agents_html.join("<hr style='margin:4px 0;border:none;border-top:1px solid #eee;'>")
        ));
    }

    if rows.is_empty() {
        rows = r#"<tr><td colspan="7" style="text-align:center;color:#666;">No workers connected</td></tr>"#.to_string();
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>TaskRun Workers</title>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 20px; background: #f5f5f5; }}
        h1 {{ color: #333; }}
        table {{ border-collapse: collapse; width: 100%; background: white; box-shadow: 0 1px 3px rgba(0,0,0,0.1); }}
        th, td {{ padding: 12px; text-align: left; border-bottom: 1px solid #eee; }}
        th {{ background: #f8f9fa; font-weight: 600; color: #555; }}
        tr:hover {{ background: #f8f9fa; }}
        small {{ color: #888; }}
        .refresh {{ color: #0066cc; text-decoration: none; margin-left: 20px; }}
        .refresh:hover {{ text-decoration: underline; }}
    </style>
</head>
<body>
    <h1>TaskRun Workers <a href="/ui/workers" class="refresh">↻ Refresh</a></h1>
    <p>Connected workers: <strong>{}</strong></p>
    <table>
        <thead>
            <tr>
                <th>Worker ID</th>
                <th>Hostname</th>
                <th>Version</th>
                <th>Status</th>
                <th>Runs</th>
                <th>Last Heartbeat</th>
                <th>Agents</th>
            </tr>
        </thead>
        <tbody>
            {}
        </tbody>
    </table>
    <p style="margin-top:20px;color:#888;font-size:12px;">
        JSON API: <a href="/v1/workers">/v1/workers</a> |
        Metrics: <a href="/metrics">/metrics</a>
    </p>
</body>
</html>"#,
        workers.len(),
        rows
    );

    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html)
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
