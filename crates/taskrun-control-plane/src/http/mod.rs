//! HTTP server for the control plane.
//!
//! Provides endpoints for:
//! - Worker enrollment (`/v1/enroll`)
//! - Worker list API (`/v1/workers`)
//! - Workers UI (`/ui/workers`)
//! - Health check (`/health`)
//! - Prometheus metrics (`/metrics`)

use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};

use crate::state::AppState;

mod handlers;
pub mod responses;


/// Create the HTTP router.
pub fn create_router(state: Arc<AppState>) -> Router {
    // CORS layer for devtools access
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // API routes
        .route("/v1/enroll", post(handlers::enroll))
        .route("/v1/workers", get(handlers::list_workers_json))
        // UI routes
        .route("/ui/workers", get(handlers::list_workers_html))
        // Observability routes
        .route("/health", get(handlers::health_check))
        .route("/metrics", get(handlers::metrics_handler))
        .layer(cors)
        .with_state(state)
}
