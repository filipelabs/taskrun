//! API response types matching the control plane.

use serde::{Deserialize, Serialize};

/// Response for a single worker.
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentResponse {
    pub name: String,
    pub description: String,
    pub backends: Vec<BackendResponse>,
}

/// Model backend information.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackendResponse {
    pub provider: String,
    pub model_name: String,
}

/// Health check response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthResponse {
    pub status: String,
}

/// Parsed metrics from Prometheus format.
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    pub workers_idle: u32,
    pub workers_busy: u32,
    pub workers_draining: u32,
    pub workers_error: u32,
    pub tasks_pending: u32,
    pub tasks_running: u32,
    pub tasks_completed: u32,
    pub tasks_failed: u32,
    pub tasks_cancelled: u32,
}

impl Metrics {
    /// Parse Prometheus text format into Metrics struct.
    pub fn from_prometheus(text: &str) -> Self {
        let mut metrics = Self::default();

        for line in text.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            // Parse lines like: taskrun_workers_connected{status="idle"} 1
            if let Some((key, value)) = line.rsplit_once(' ') {
                if let Ok(v) = value.parse::<u32>() {
                    if key.contains("workers_connected") {
                        if key.contains("idle") {
                            metrics.workers_idle = v;
                        } else if key.contains("busy") {
                            metrics.workers_busy = v;
                        } else if key.contains("draining") {
                            metrics.workers_draining = v;
                        } else if key.contains("error") {
                            metrics.workers_error = v;
                        }
                    } else if key.contains("tasks_total") {
                        if key.contains("pending") {
                            metrics.tasks_pending = v;
                        } else if key.contains("running") {
                            metrics.tasks_running = v;
                        } else if key.contains("completed") {
                            metrics.tasks_completed = v;
                        } else if key.contains("failed") {
                            metrics.tasks_failed = v;
                        } else if key.contains("cancelled") {
                            metrics.tasks_cancelled = v;
                        }
                    }
                }
            }
        }

        metrics
    }

    /// Total number of workers.
    pub fn total_workers(&self) -> u32 {
        self.workers_idle + self.workers_busy + self.workers_draining + self.workers_error
    }

    /// Total number of tasks.
    pub fn total_tasks(&self) -> u32 {
        self.tasks_pending + self.tasks_running + self.tasks_completed + self.tasks_failed + self.tasks_cancelled
    }
}
