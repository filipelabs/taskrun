//! Task scheduler - routes tasks to workers.

use std::sync::Arc;

use thiserror::Error;
use tracing::{info, warn};

use taskrun_core::{RunId, RunSummary, TaskId, TaskStatus, WorkerId};
use taskrun_proto::pb::run_server_message::Payload as ServerPayload;
use taskrun_proto::pb::{RunAssignment, RunServerMessage};

use crate::state::AppState;

/// Scheduler errors.
#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("Task not found: {0}")]
    TaskNotFound(TaskId),

    #[error("No workers available for agent: {0}")]
    NoWorkersAvailable(String),

    #[error("Failed to send assignment to worker: {0}")]
    SendFailed(String),
}

/// Task scheduler.
pub struct Scheduler {
    state: Arc<AppState>,
}

impl Scheduler {
    /// Create a new Scheduler.
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Select a worker that supports the given agent and has capacity.
    #[allow(dead_code)]
    pub async fn select_worker(&self, agent_name: &str) -> Option<WorkerId> {
        let workers = self.state.workers.read().await;

        for (worker_id, worker) in workers.iter() {
            // Check if worker supports this agent
            if !worker.info.supports_agent(agent_name) {
                continue;
            }

            // Check if worker has capacity
            if worker.active_runs >= worker.max_concurrent_runs {
                continue;
            }

            // Check if worker can accept runs
            if !worker.status.can_accept_runs() {
                continue;
            }

            return Some(worker_id.clone());
        }

        None
    }

    /// Assign a task to a worker, creating a Run.
    pub async fn assign_task(&self, task_id: &TaskId) -> Result<RunId, SchedulerError> {
        // Get task
        let mut tasks = self.state.tasks.write().await;
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| SchedulerError::TaskNotFound(task_id.clone()))?;

        // Find a suitable worker
        let worker_id = {
            let workers = self.state.workers.read().await;

            let mut selected: Option<WorkerId> = None;
            for (wid, worker) in workers.iter() {
                if worker.info.supports_agent(&task.agent_name)
                    && worker.active_runs < worker.max_concurrent_runs
                    && worker.status.can_accept_runs()
                {
                    selected = Some(wid.clone());
                    break;
                }
            }
            selected
        }
        .ok_or_else(|| SchedulerError::NoWorkersAvailable(task.agent_name.clone()))?;

        // Create run summary
        let mut run = RunSummary::new(worker_id.clone());
        let run_id = run.run_id.clone();

        // Mark as assigned
        run.status = taskrun_core::RunStatus::Assigned;

        // Add run to task
        task.runs.push(run);
        task.status = TaskStatus::Running;

        info!(
            task_id = %task_id,
            run_id = %run_id,
            worker_id = %worker_id,
            agent = %task.agent_name,
            "Assigning task to worker"
        );

        // Build assignment message
        let assignment = RunAssignment {
            run_id: run_id.as_str().to_string(),
            task_id: task_id.as_str().to_string(),
            agent_name: task.agent_name.clone(),
            input_json: task.input_json.clone(),
            labels: task.labels.clone(),
            issued_at_ms: chrono::Utc::now().timestamp_millis(),
            deadline_ms: 0, // No deadline for now
        };

        let msg = RunServerMessage {
            payload: Some(ServerPayload::AssignRun(assignment)),
        };

        // Drop task lock before acquiring worker lock
        drop(tasks);

        // Send to worker
        {
            let mut workers = self.state.workers.write().await;
            if let Some(worker) = workers.get_mut(&worker_id) {
                worker.active_runs += 1;

                if worker.tx.send(msg).await.is_err() {
                    warn!(worker_id = %worker_id, "Failed to send assignment - worker disconnected");
                    return Err(SchedulerError::SendFailed(worker_id.to_string()));
                }
            } else {
                return Err(SchedulerError::SendFailed(format!(
                    "Worker {} disappeared",
                    worker_id
                )));
            }
        }

        Ok(run_id)
    }
}
