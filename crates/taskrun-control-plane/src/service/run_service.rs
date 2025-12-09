//! RunService implementation for the control plane.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status, Streaming};
use tracing::{error, info, warn};

use taskrun_core::{
    ChatMessage, ChatRole, RunEvent, RunEventType, RunId, RunStatus, TaskId, TaskStatus, WorkerId,
    WorkerInfo, WorkerStatus,
};
use taskrun_proto::pb::run_client_message::Payload as ClientPayload;
use taskrun_proto::pb::{
    RunChatMessage, RunClientMessage, RunEvent as ProtoRunEvent, RunOutputChunk, RunServerMessage,
    RunStatusUpdate, WorkerHeartbeat, WorkerHello,
};
use taskrun_proto::{RunService, RunServiceServer};

use crate::service::mtls::validate_worker_id_format;
use crate::state::{AppState, ConnectedWorker, StreamEvent, UiNotification};

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
                                ClientPayload::Event(event) => {
                                    handle_event(&state_clone, event).await;
                                }
                                ClientPayload::ChatMessage(chat_msg) => {
                                    handle_chat_message(&state_clone, chat_msg).await;
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

                // Notify UI
                state_clone.notify_ui(UiNotification::WorkerDisconnected { worker_id: id });
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

        // Capture info for notification before move
        let hostname = info.hostname.clone();
        let agents: Vec<String> = info.agents.iter().map(|a| a.name.clone()).collect();

        // Register worker in state
        let connected = ConnectedWorker {
            info,
            status: WorkerStatus::Idle,
            active_runs: 0,
            max_concurrent_runs: 10,
            last_heartbeat: chrono::Utc::now(),
            tx,
        };

        state
            .workers
            .write()
            .await
            .insert(worker_id.clone(), connected);

        // Notify UI
        state.notify_ui(UiNotification::WorkerConnected {
            worker_id,
            hostname,
            agents,
        });
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

        // Notify UI
        drop(workers); // Release lock before notification
        state.notify_ui(UiNotification::WorkerHeartbeat {
            worker_id,
            status,
            active_runs: hb.active_runs,
            max_concurrent_runs: hb.max_concurrent_runs,
        });
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

                // Capture for stream event before releasing locks
                let error_msg = if !update.error_message.is_empty() {
                    Some(update.error_message.clone())
                } else {
                    None
                };
                let timestamp = update.timestamp_ms;
                let is_terminal = run_status.is_terminal();

                // Decrement worker's active_runs if terminal
                if is_terminal {
                    drop(tasks); // Release lock before acquiring workers lock
                    let mut workers = state.workers.write().await;
                    if let Some(worker) = workers.get_mut(&worker_id) {
                        if worker.active_runs > 0 {
                            worker.active_runs -= 1;
                        }
                    }
                    drop(workers);
                } else {
                    drop(tasks);
                }

                // Publish stream event for SSE subscribers
                state
                    .publish_stream_event(
                        &run_id,
                        StreamEvent::StatusUpdate {
                            status: run_status,
                            error_message: error_msg,
                            timestamp_ms: timestamp,
                        },
                    )
                    .await;

                // Notify UI of run status change
                state.notify_ui(UiNotification::RunStatusChanged {
                    run_id: run_id.clone(),
                    task_id: task_id.clone(),
                    worker_id: Some(worker_id.clone()),
                    status: run_status,
                });

                // Notify UI of task status change if it changed
                if run_status.is_terminal() || run_status == RunStatus::Running {
                    let task_status = match run_status {
                        RunStatus::Running => TaskStatus::Running,
                        RunStatus::Completed => TaskStatus::Completed,
                        RunStatus::Failed => TaskStatus::Failed,
                        RunStatus::Cancelled => TaskStatus::Cancelled,
                        _ => return,
                    };
                    state.notify_ui(UiNotification::TaskStatusChanged {
                        task_id: task_id.clone(),
                        status: task_status,
                    });
                }

                // Schedule cleanup for terminal status
                if is_terminal {
                    let state_clone = state.clone();
                    let run_id_clone = run_id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        state_clone.remove_stream_channel(&run_id_clone).await;
                    });
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

    if let Some(ref task_id) = task_id {
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

    // Store output content (append to existing output for this run)
    if !chunk.content.is_empty() {
        state.append_output(&run_id, &chunk.content).await;
    }

    // Publish to stream channel for SSE subscribers
    let content_for_ui = chunk.content.clone();
    state
        .publish_stream_event(
            &run_id,
            StreamEvent::OutputChunk {
                seq: chunk.seq,
                content: chunk.content,
                is_final: chunk.is_final,
                timestamp_ms: chunk.timestamp_ms,
            },
        )
        .await;

    // Notify UI of output chunk
    if let Some(task_id) = task_id {
        state.notify_ui(UiNotification::RunOutputChunk {
            run_id,
            task_id,
            content: content_for_ui,
        });
    }
}

async fn handle_event(state: &Arc<AppState>, proto_event: ProtoRunEvent) {
    // Convert proto event type to domain event type
    let event_type = match taskrun_proto::pb::RunEventType::try_from(proto_event.event_type) {
        Ok(taskrun_proto::pb::RunEventType::ExecutionStarted) => RunEventType::ExecutionStarted,
        Ok(taskrun_proto::pb::RunEventType::SessionInitialized) => RunEventType::SessionInitialized,
        Ok(taskrun_proto::pb::RunEventType::ToolRequested) => RunEventType::ToolRequested,
        Ok(taskrun_proto::pb::RunEventType::ToolCompleted) => RunEventType::ToolCompleted,
        Ok(taskrun_proto::pb::RunEventType::OutputGenerated) => RunEventType::OutputGenerated,
        Ok(taskrun_proto::pb::RunEventType::ExecutionCompleted) => RunEventType::ExecutionCompleted,
        Ok(taskrun_proto::pb::RunEventType::ExecutionFailed) => RunEventType::ExecutionFailed,
        _ => {
            warn!(event_id = %proto_event.id, "Unknown event type");
            return;
        }
    };

    // Convert to domain event
    let event = RunEvent {
        id: proto_event.id.clone().into(),
        run_id: RunId::new(&proto_event.run_id),
        task_id: TaskId::new(&proto_event.task_id),
        event_type,
        timestamp_ms: proto_event.timestamp_ms,
        metadata: proto_event.metadata.clone(),
    };

    info!(
        event_id = %proto_event.id,
        run_id = %proto_event.run_id,
        task_id = %proto_event.task_id,
        event_type = ?event_type,
        "Run event received"
    );

    // Notify UI
    state.notify_ui(UiNotification::RunEvent {
        run_id: event.run_id.clone(),
        task_id: event.task_id.clone(),
        event_type,
    });

    // Store the event
    state.store_event(event).await;
}

async fn handle_chat_message(state: &Arc<AppState>, chat_msg: RunChatMessage) {
    let run_id = RunId::new(&chat_msg.run_id);

    // Parse the message if present
    let proto_msg = match chat_msg.message {
        Some(msg) => msg,
        None => {
            warn!(run_id = %chat_msg.run_id, "Chat message received without content");
            return;
        }
    };

    // Convert proto role to domain role
    let role = match taskrun_proto::pb::ChatRole::try_from(proto_msg.role) {
        Ok(taskrun_proto::pb::ChatRole::User) => ChatRole::User,
        Ok(taskrun_proto::pb::ChatRole::Assistant) => ChatRole::Assistant,
        Ok(taskrun_proto::pb::ChatRole::System) => ChatRole::System,
        _ => {
            warn!(run_id = %chat_msg.run_id, role = proto_msg.role, "Unknown chat role");
            return;
        }
    };

    // Find task_id for this run
    let task_id = {
        let tasks = state.tasks.read().await;
        tasks
            .values()
            .find(|t| t.runs.iter().any(|r| r.run_id == run_id))
            .map(|t| t.id.clone())
    };

    let task_id = match task_id {
        Some(id) => id,
        None => {
            warn!(run_id = %chat_msg.run_id, "Chat message for unknown run");
            return;
        }
    };

    info!(
        run_id = %chat_msg.run_id,
        task_id = %task_id,
        role = ?role,
        content_len = proto_msg.content.len(),
        "Chat message received"
    );

    // Create domain chat message
    let message = ChatMessage {
        role,
        content: proto_msg.content.clone(),
        timestamp_ms: proto_msg.timestamp_ms,
    };

    // Store the message
    state.store_chat_message(&run_id, message).await;

    // Notify UI
    state.notify_ui(UiNotification::ChatMessage {
        run_id,
        task_id,
        role,
        content: proto_msg.content,
    });
}
