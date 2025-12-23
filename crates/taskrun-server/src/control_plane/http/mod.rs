//! HTTP server for the control plane.
//!
//! Provides endpoints for:
//! - OpenAI-compatible responses API (`/v1/responses`)
//! - Worker enrollment (`/v1/enroll`)
//! - Worker list API (`/v1/workers`)
//! - Workers UI (`/ui/workers`)
//! - Health check (`/health`)
//! - Prometheus metrics (`/metrics`)
//! - MCP tools (`/mcp/tools/*`)

use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};

use crate::control_plane::state::AppState;

mod handlers;
mod mcp;
pub mod responses;

/// Create the HTTP router.
pub fn create_router(state: Arc<AppState>) -> Router {
    // CORS layer for devtools access
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // OpenAI-compatible API
        .route("/v1/responses", post(handlers::create_response))
        // API routes
        .route("/v1/enroll", post(handlers::enroll))
        .route("/v1/workers", get(handlers::list_workers_json))
        .route("/v1/tasks/:task_id/events", get(handlers::get_task_events))
        .route("/v1/tasks/:task_id/output", get(handlers::get_task_output))
        // MCP tools
        .route("/mcp/tools/list_workers", post(mcp::list_workers))
        .route("/mcp/tools/start_new_task", post(mcp::start_new_task))
        .route("/mcp/tools/read_task", post(mcp::read_task))
        .route("/mcp/tools/continue_task", post(mcp::continue_task))
        // UI routes
        .route("/ui/workers", get(handlers::list_workers_html))
        // Observability routes
        .route("/health", get(handlers::health_check))
        .route("/metrics", get(handlers::metrics_handler))
        .layer(cors)
        .with_state(state)
}
