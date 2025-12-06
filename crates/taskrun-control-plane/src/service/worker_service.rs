//! WorkerService implementation - query worker state and capabilities.

use std::sync::Arc;

use tonic::{Request, Response, Status};

use taskrun_proto::pb::{GetWorkerRequest, ListWorkersRequest, ListWorkersResponse, Worker};
use taskrun_proto::{WorkerService, WorkerServiceServer};

use crate::state::{AppState, ConnectedWorker};

/// gRPC WorkerService implementation.
pub struct WorkerServiceImpl {
    state: Arc<AppState>,
}

impl WorkerServiceImpl {
    /// Create a new WorkerServiceImpl.
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Convert into a tonic server.
    pub fn into_server(self) -> WorkerServiceServer<Self> {
        WorkerServiceServer::new(self)
    }
}

/// Convert ConnectedWorker to proto Worker message.
fn connected_worker_to_proto(worker: &ConnectedWorker) -> Worker {
    Worker {
        worker_id: worker.info.worker_id.as_str().to_string(),
        hostname: worker.info.hostname.clone(),
        version: worker.info.version.clone(),
        status: taskrun_proto::pb::WorkerStatus::from(worker.status) as i32,
        agents: worker.info.agents.iter().cloned().map(Into::into).collect(),
        labels: worker.info.labels.clone(),
        active_runs: worker.active_runs,
        max_concurrent_runs: worker.max_concurrent_runs,
        last_heartbeat_ms: worker.last_heartbeat.timestamp_millis(),
    }
}

#[tonic::async_trait]
impl WorkerService for WorkerServiceImpl {
    async fn list_workers(
        &self,
        request: Request<ListWorkersRequest>,
    ) -> Result<Response<ListWorkersResponse>, Status> {
        let req = request.into_inner();
        let workers = self.state.workers.read().await;

        let mut result: Vec<Worker> = Vec::new();

        for worker in workers.values() {
            // Filter by agent_name if specified
            if let Some(ref agent_name) = req.agent_name {
                if !worker.info.supports_agent(agent_name) {
                    continue;
                }
            }

            // Filter by status if specified
            if let Some(status_filter) = req.status {
                let worker_status = taskrun_proto::pb::WorkerStatus::from(worker.status) as i32;
                if worker_status != status_filter {
                    continue;
                }
            }

            result.push(connected_worker_to_proto(worker));
        }

        Ok(Response::new(ListWorkersResponse { workers: result }))
    }

    async fn get_worker(
        &self,
        request: Request<GetWorkerRequest>,
    ) -> Result<Response<Worker>, Status> {
        let req = request.into_inner();
        let worker_id = taskrun_core::WorkerId::new(req.worker_id.clone());

        let workers = self.state.workers.read().await;

        match workers.get(&worker_id) {
            Some(worker) => Ok(Response::new(connected_worker_to_proto(worker))),
            None => Err(Status::not_found(format!(
                "Worker {} not found",
                req.worker_id
            ))),
        }
    }
}
