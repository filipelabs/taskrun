//! RunService implementation for the control plane.

use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status, Streaming};
use tracing::{error, info, warn};

use taskrun_core::{RunId, RunStatus, TaskStatus, WorkerId, WorkerInfo, WorkerStatus};
use taskrun_proto::pb::run_client_message::Payload as ClientPayload;
use taskrun_proto::pb::{
    RunClientMessage, RunOutputChunk, RunServerMessage, RunStatusUpdate, WorkerHeartbeat,
    WorkerHello,
};
use taskrun_proto::{RunService, RunServiceServer};

use crate::service::mtls::validate_worker_id_format;
use crate::state::{AppState, ConnectedWorker};

/// RunService implementation.
pub struct RunServiceImpl {
    state: Arc<AppState>,
}

impl RunServiceImpl {
    /// Create a new RunServiceImpl.
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Convert into a tonic server.
    pub fn into_server(self) -> RunServiceServer<Self> {
        RunServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl RunService for RunServiceImpl {
    type StreamConnectStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<RunServerMessage, Status>> + Send>>;

    async fn stream_connect(
        &self,
        request: Request<Streaming<RunClientMessage>>,
    ) -> Result<Response<Self::StreamConnectStream>, Status> {
        let mut inbound = request.into_inner();
        let state = self.state.clone();

        // Create channel for outbound messages to worker
        let (tx, rx) = mpsc::channel::<RunServerMessage>(32);

        // Track worker_id once we receive WorkerHello
        let worker_id: Arc<Mutex<Option<WorkerId>>> = Arc::new(Mutex::new(None));
        let worker_id_clone = worker_id.clone();
        let state_clone = state.clone();
        let tx_clone = tx.clone();

        // Spawn task to process incoming messages
        tokio::spawn(async move {
            while let Some(result) = inbound.next().await {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload {
                            match payload {
                                ClientPayload::Hello(hello) => {
                                    handle_worker_hello(
                                        &state_clone,
                                        &worker_id_clone,
                                        hello,
                                        tx_clone.clone(),
                                    )
                                    .await;
                                }
                                ClientPayload::Heartbeat(hb) => {
                                    handle_heartbeat(&state_clone, hb).await;
                                }
                                ClientPayload::StatusUpdate(update) => {
                                    handle_status_update(&state_clone, update).await;
                                }
                                ClientPayload::OutputChunk(chunk) => {
                                    handle_output_chunk(&state_clone, chunk).await;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Stream error");
                        break;
                    }
                }
            }

            // Worker disconnected - clean up
            if let Some(id) = worker_id_clone.lock().await.take() {
                info!(worker_id = %id, "Worker disconnected");
                state_clone.workers.write().await.remove(&id);
            }
        });

        // Convert receiver to stream
        let outbound = ReceiverStream::new(rx).map(Ok);

        Ok(Response::new(Box::pin(outbound)))
    }
}

async fn handle_worker_hello(
    state: &Arc<AppState>,
    worker_id_holder: &Arc<Mutex<Option<WorkerId>>>,
    hello: WorkerHello,
    tx: mpsc::Sender<RunServerMessage>,
) {
    if let Some(info_proto) = hello.info {
        let info: WorkerInfo = info_proto.into();
        let worker_id = info.worker_id.clone();

        // Validate worker_id format (mTLS ensures the worker has a valid cert)
        if let Err(e) = validate_worker_id_format(&worker_id) {
            error!(
                worker_id = %worker_id,
                error = %e,
                "Worker ID validation failed"
            );
            return;
        }

        let agent_names: Vec<&str> = info.agents.iter().map(|a| a.name.as_str()).collect();
        info!(
            worker_id = %worker_id,
            hostname = %info.hostname,
            version = %info.version,
            agents = ?agent_names,
            "Worker authenticated via mTLS"
        );

        // Store worker_id for cleanup on disconnect
        *worker_id_holder.lock().await = Some(worker_id.clone());

        // Register worker in state
        let connected = ConnectedWorker {
            info,
            status: WorkerStatus::Idle,
            active_runs: 0,
            max_concurrent_runs: 10,
            last_heartbeat: chrono::Utc::now(),
            tx,
        };

        state.workers.write().await.insert(worker_id, connected);
    } else {
        error!("WorkerHello received without WorkerInfo");
    }
}

async fn handle_heartbeat(state: &Arc<AppState>, hb: WorkerHeartbeat) {
    let worker_id = WorkerId::new(&hb.worker_id);

    let mut workers = state.workers.write().await;
    if let Some(worker) = workers.get_mut(&worker_id) {
        // Convert proto status to domain status
        let status = match taskrun_proto::pb::WorkerStatus::try_from(hb.status) {
            Ok(taskrun_proto::pb::WorkerStatus::Idle) => WorkerStatus::Idle,
            Ok(taskrun_proto::pb::WorkerStatus::Busy) => WorkerStatus::Busy,
            Ok(taskrun_proto::pb::WorkerStatus::Draining) => WorkerStatus::Draining,
            Ok(taskrun_proto::pb::WorkerStatus::Error) => WorkerStatus::Error,
            _ => WorkerStatus::Idle,
        };

        worker.status = status;
        worker.active_runs = hb.active_runs;
        worker.max_concurrent_runs = hb.max_concurrent_runs;
        worker.last_heartbeat = chrono::Utc::now();

        info!(
            worker_id = %worker_id,
            status = ?worker.status,
            active_runs = worker.active_runs,
            "Heartbeat received"
        );
    } else {
        warn!(worker_id = %hb.worker_id, "Heartbeat from unknown worker");
    }
}

async fn handle_status_update(state: &Arc<AppState>, update: RunStatusUpdate) {
    let run_id = RunId::new(&update.run_id);

    // Convert proto status to domain status
    let run_status: RunStatus = taskrun_proto::pb::RunStatus::try_from(update.status)
        .unwrap_or(taskrun_proto::pb::RunStatus::Unspecified)
        .into();

    // Find the task containing this run and update it
    let mut tasks = state.tasks.write().await;
    for task in tasks.values_mut() {
        for run in &mut task.runs {
            if run.run_id == run_id {
                // Log with full correlation
                info!(
                    task_id = %task.id,
                    run_id = %run_id,
                    worker_id = %run.worker_id,
                    status = ?run_status,
                    "Run status update"
                );

                run.status = run_status;

                // Update timestamps
                if run_status == RunStatus::Running {
                    run.started_at = Some(chrono::Utc::now());
                } else if run_status.is_terminal() {
                    run.finished_at = Some(chrono::Utc::now());
                }

                // Update error message if present
                if !update.error_message.is_empty() {
                    run.error_message = Some(update.error_message.clone());
                }

                // Update backend_used if present
                if let Some(backend) = &update.backend_used {
                    run.backend_used = Some(backend.clone().into());
                }

                // Capture worker_id before we might need to drop the lock
                let worker_id = run.worker_id.clone();
                let task_id = task.id.clone();

                // Update task status based on run status
                match run_status {
                    RunStatus::Running => {
                        if task.status == TaskStatus::Pending {
                            task.status = TaskStatus::Running;
                        }
                    }
                    RunStatus::Completed => {
                        task.status = TaskStatus::Completed;
                        info!(task_id = %task_id, run_id = %run_id, "Task completed");
                    }
                    RunStatus::Failed => {
                        task.status = TaskStatus::Failed;
                        info!(task_id = %task_id, run_id = %run_id, "Task failed");
                    }
                    RunStatus::Cancelled => {
                        task.status = TaskStatus::Cancelled;
                        info!(task_id = %task_id, run_id = %run_id, "Task cancelled");
                    }
                    _ => {}
                }

                // Decrement worker's active_runs if terminal
                if run_status.is_terminal() {
                    drop(tasks); // Release lock before acquiring workers lock
                    let mut workers = state.workers.write().await;
                    if let Some(worker) = workers.get_mut(&worker_id) {
                        if worker.active_runs > 0 {
                            worker.active_runs -= 1;
                        }
                    }
                }

                return;
            }
        }
    }

    warn!(run_id = %update.run_id, "Status update for unknown run");
}

async fn handle_output_chunk(state: &Arc<AppState>, chunk: RunOutputChunk) {
    let run_id = RunId::new(&chunk.run_id);

    // Find task_id for correlation
    let task_id = {
        let tasks = state.tasks.read().await;
        tasks
            .values()
            .find(|t| t.runs.iter().any(|r| r.run_id == run_id))
            .map(|t| t.id.clone())
    };

    if let Some(task_id) = task_id {
        info!(
            task_id = %task_id,
            run_id = %chunk.run_id,
            seq = chunk.seq,
            is_final = chunk.is_final,
            content_len = chunk.content.len(),
            "Output chunk received"
        );
    } else {
        warn!(
            run_id = %chunk.run_id,
            seq = chunk.seq,
            "Output chunk for unknown run"
        );
    }

    // For now, just log the chunk. In a real implementation, we would:
    // - Store output chunks for later retrieval
    // - Stream to connected clients watching the task
    // - Aggregate for final output
}
