//! TaskService implementation for the control plane.

use std::sync::Arc;

use tonic::{Request, Response, Status};
use tracing::{info, warn};

use taskrun_core::{RunStatus, Task, TaskId, TaskStatus};
use taskrun_proto::pb::run_server_message::Payload as ServerPayload;
use taskrun_proto::pb::{
    CancelRun, CancelTaskRequest, CreateTaskRequest, GetTaskRequest, ListTasksRequest,
    ListTasksResponse, RunServerMessage,
};
use taskrun_proto::{TaskService, TaskServiceServer};

use crate::control_plane::scheduler::Scheduler;
use crate::control_plane::state::{AppState, UiNotification};

/// TaskService implementation.
pub struct TaskServiceImpl {
    state: Arc<AppState>,
    scheduler: Scheduler,
}

impl TaskServiceImpl {
    /// Create a new TaskServiceImpl.
    pub fn new(state: Arc<AppState>) -> Self {
        let scheduler = Scheduler::new(state.clone());
        Self { state, scheduler }
    }

    /// Convert into a tonic server.
    pub fn into_server(self) -> TaskServiceServer<Self> {
        TaskServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl TaskService for TaskServiceImpl {
    async fn create_task(
        &self,
        request: Request<CreateTaskRequest>,
    ) -> Result<Response<taskrun_proto::pb::Task>, Status> {
        let req = request.into_inner();

        // Validate request
        if req.agent_name.is_empty() {
            return Err(Status::invalid_argument("agent_name is required"));
        }

        // Create task
        let mut task = Task::new(&req.agent_name, &req.input_json, &req.created_by);
        for (k, v) in req.labels {
            task.labels.insert(k, v);
        }

        let task_id = task.id.clone();

        info!(
            task_id = %task_id,
            agent = %req.agent_name,
            created_by = %req.created_by,
            "Creating task"
        );

        // Store task
        self.state.tasks.write().await.insert(task_id.clone(), task);

        // Notify UI
        self.state.notify_ui(UiNotification::TaskCreated {
            task_id: task_id.clone(),
            agent: req.agent_name.clone(),
        });

        // Try to schedule immediately
        match self.scheduler.assign_task(&task_id).await {
            Ok(run_id) => {
                info!(task_id = %task_id, run_id = %run_id, "Task assigned to worker");
            }
            Err(e) => {
                warn!(task_id = %task_id, error = %e, "Failed to assign task (no workers available?)");
                // Task stays PENDING, could be picked up later
            }
        }

        // Return current task state
        let task = self
            .state
            .tasks
            .read()
            .await
            .get(&task_id)
            .cloned()
            .ok_or_else(|| Status::internal("Task disappeared after creation"))?;

        Ok(Response::new(task.into()))
    }

    async fn get_task(
        &self,
        request: Request<GetTaskRequest>,
    ) -> Result<Response<taskrun_proto::pb::Task>, Status> {
        let req = request.into_inner();
        let task_id = TaskId::new(&req.id);

        let task = self
            .state
            .tasks
            .read()
            .await
            .get(&task_id)
            .cloned()
            .ok_or_else(|| Status::not_found(format!("Task not found: {}", req.id)))?;

        Ok(Response::new(task.into()))
    }

    async fn list_tasks(
        &self,
        request: Request<ListTasksRequest>,
    ) -> Result<Response<ListTasksResponse>, Status> {
        let req = request.into_inner();
        let limit = if req.limit > 0 {
            req.limit as usize
        } else {
            100
        };

        let tasks = self.state.tasks.read().await;

        let filtered: Vec<taskrun_proto::pb::Task> = tasks
            .values()
            .filter(|task| {
                // Status filter
                if req.status_filter != 0 {
                    let filter_status: TaskStatus =
                        taskrun_proto::pb::TaskStatus::try_from(req.status_filter)
                            .unwrap_or(taskrun_proto::pb::TaskStatus::Unspecified)
                            .into();
                    if task.status != filter_status {
                        return false;
                    }
                }
                // Agent filter
                if !req.agent_filter.is_empty() && task.agent_name != req.agent_filter {
                    return false;
                }
                true
            })
            .take(limit)
            .cloned()
            .map(Into::into)
            .collect();

        Ok(Response::new(ListTasksResponse { tasks: filtered }))
    }

    async fn cancel_task(
        &self,
        request: Request<CancelTaskRequest>,
    ) -> Result<Response<taskrun_proto::pb::Task>, Status> {
        let req = request.into_inner();
        let task_id = TaskId::new(&req.id);

        // Collect runs to cancel (worker_id, run_id pairs)
        let runs_to_cancel: Vec<_>;
        let result_task: Task;

        {
            let mut tasks = self.state.tasks.write().await;
            let task = tasks
                .get_mut(&task_id)
                .ok_or_else(|| Status::not_found(format!("Task not found: {}", req.id)))?;

            // Check if cancellable
            if task.is_terminal() {
                return Err(Status::failed_precondition(format!(
                    "Task {} is already in terminal state: {:?}",
                    req.id, task.status
                )));
            }

            info!(task_id = %task_id, "Cancelling task");

            // Notify UI of task status change
            self.state.notify_ui(UiNotification::TaskStatusChanged {
                task_id: task_id.clone(),
                status: TaskStatus::Cancelled,
            });

            // Collect active runs
            runs_to_cancel = task
                .runs
                .iter()
                .filter(|r| r.status.is_active())
                .map(|r| (r.worker_id.clone(), r.run_id.clone()))
                .collect();

            // Mark task and its runs as cancelled
            task.status = TaskStatus::Cancelled;
            for run in &mut task.runs {
                if run.status.is_active() {
                    run.status = RunStatus::Cancelled;
                    run.finished_at = Some(chrono::Utc::now());
                }
            }

            result_task = task.clone();
        }

        // Send CancelRun to workers (outside the tasks lock)
        if !runs_to_cancel.is_empty() {
            let workers = self.state.workers.read().await;
            for (worker_id, run_id) in &runs_to_cancel {
                if let Some(worker) = workers.get(worker_id) {
                    let cancel_msg = RunServerMessage {
                        payload: Some(ServerPayload::CancelRun(CancelRun {
                            run_id: run_id.to_string(),
                            reason: "Task cancelled by user".to_string(),
                        })),
                    };

                    if let Err(e) = worker.tx.send(cancel_msg).await {
                        warn!(
                            task_id = %task_id,
                            run_id = %run_id,
                            worker_id = %worker_id,
                            error = %e,
                            "Failed to send CancelRun to worker"
                        );
                    } else {
                        info!(
                            task_id = %task_id,
                            run_id = %run_id,
                            worker_id = %worker_id,
                            "Sent CancelRun to worker"
                        );
                    }
                } else {
                    warn!(
                        task_id = %task_id,
                        run_id = %run_id,
                        worker_id = %worker_id,
                        "Worker not connected, cannot send CancelRun"
                    );
                }
            }
        }

        Ok(Response::new(result_task.into()))
    }
}
