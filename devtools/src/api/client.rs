//! HTTP client for TaskRun control plane API.

use gloo_net::http::Request;

use super::types::{EventResponse, HealthResponse, Metrics, OutputResponse, WorkerResponse};

/// Base URL for the control plane HTTP API.
const BASE_URL: &str = "http://[::1]:50052";

/// Fetch health status from control plane.
pub async fn fetch_health() -> Result<HealthResponse, String> {
    Request::get(&format!("{}/health", BASE_URL))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

/// Fetch list of connected workers.
pub async fn fetch_workers() -> Result<Vec<WorkerResponse>, String> {
    Request::get(&format!("{}/v1/workers", BASE_URL))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

/// Fetch and parse Prometheus metrics.
pub async fn fetch_metrics() -> Result<Metrics, String> {
    let text = Request::get(&format!("{}/metrics", BASE_URL))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    Ok(Metrics::from_prometheus(&text))
}

/// Fetch events for a specific task.
pub async fn fetch_task_events(task_id: &str) -> Result<Vec<EventResponse>, String> {
    Request::get(&format!("{}/v1/tasks/{}/events", BASE_URL, task_id))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

/// Fetch output for a specific task.
pub async fn fetch_task_output(task_id: &str) -> Result<OutputResponse, String> {
    Request::get(&format!("{}/v1/tasks/{}/output", BASE_URL, task_id))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}
