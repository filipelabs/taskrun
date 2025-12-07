//! Health and metrics handlers.

use std::sync::Arc;

use axum::{extract::State, http::header, response::IntoResponse, Json};

use crate::state::AppState;

/// Health check endpoint.
pub async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Prometheus metrics endpoint.
pub async fn metrics_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let body = crate::metrics::collect_metrics(&state).await;
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body)
}
