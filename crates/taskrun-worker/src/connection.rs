//! Connection management for the worker.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use tracing::{error, info, warn};

use taskrun_core::{AgentSpec, ModelBackend, RunEvent, RunId, TaskId, WorkerInfo};
use taskrun_proto::pb::run_client_message::Payload as ClientPayload;
use taskrun_proto::pb::run_server_message::Payload as ServerPayload;
use taskrun_proto::pb::{
    ChatMessage, ChatRole as ProtoChatRole, ContinueRun, RunAssignment, RunChatMessage,
    RunClientMessage, RunEvent as ProtoRunEvent, RunOutputChunk, RunStatusUpdate, WorkerHeartbeat,
    WorkerHello,
};
use taskrun_proto::RunServiceClient;

use crate::config::Config;
use crate::executor::ClaudeCodeExecutor;
use crate::json_output;

/// Session info stored for each run.
#[derive(Debug, Clone)]
struct SessionInfo {
    session_id: String,
    task_id: String,
}

/// Manages connection to the control plane.
pub struct WorkerConnection {
    config: Arc<Config>,
    outbound_tx: Option<mpsc::Sender<RunClientMessage>>,
    active_run_count: Arc<AtomicU32>,
    executor: Arc<ClaudeCodeExecutor>,
    /// Maps run_id -> session info for session continuation.
    sessions: Arc<Mutex<HashMap<String, SessionInfo>>>,
}

impl WorkerConnection {
    /// Create a new WorkerConnection.
    pub fn new(config: Arc<Config>) -> Self {
        let executor = Arc::new(ClaudeCodeExecutor::new(config.clone()));
        Self {
            config,
            outbound_tx: None,
            active_run_count: Arc::new(AtomicU32::new(0)),
            executor,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Connect to control plane and run the main loop.
    /// Returns on disconnect (caller should handle reconnection).
    pub async fn connect_and_run(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(addr = %self.config.control_plane_addr, "Connecting to control plane with mTLS");

        // Load CA certificate for pinned trust
        let ca_cert = std::fs::read(&self.config.tls_ca_cert_path).map_err(|e| {
            format!(
                "Failed to read CA certificate from '{}': {}. Run scripts/gen-dev-certs.sh first.",
                self.config.tls_ca_cert_path, e
            )
        })?;

        // Load client certificate and key for mTLS
        let client_cert = std::fs::read(&self.config.tls_cert_path).map_err(|e| {
            format!(
                "Failed to read worker certificate from '{}': {}. Run scripts/gen-worker-cert.sh first.",
                self.config.tls_cert_path, e
            )
        })?;
        let client_key = std::fs::read(&self.config.tls_key_path).map_err(|e| {
            format!(
                "Failed to read worker key from '{}': {}. Run scripts/gen-worker-cert.sh first.",
                self.config.tls_key_path, e
            )
        })?;

        let tls_config = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(ca_cert))
            .identity(Identity::from_pem(client_cert, client_key))
            .domain_name("localhost");

        let channel = Channel::from_shared(self.config.control_plane_addr.clone())?
            .tls_config(tls_config)?
            .connect()
            .await?;

        let mut client = RunServiceClient::new(channel);

        // Create channel for outbound messages
        let (tx, rx) = mpsc::channel::<RunClientMessage>(32);
        self.outbound_tx = Some(tx.clone());

        // Convert receiver to stream for gRPC
        let outbound_stream = ReceiverStream::new(rx);

        // Start streaming connection
        let response = client.stream_connect(outbound_stream).await?;
        let mut inbound = response.into_inner();

        info!("Connected to control plane, sending WorkerHello");

        // Emit JSON event for worker connected
        json_output::emit_worker_connected(
            self.config.worker_id.as_str(),
            &self.config.control_plane_addr,
        );

        // Send WorkerHello
        self.send_hello().await?;

        // Start heartbeat task
        let heartbeat_tx = tx.clone();
        let heartbeat_config = self.config.clone();
        let heartbeat_run_count = self.active_run_count.clone();
        let heartbeat_handle = tokio::spawn(async move {
            run_heartbeat_loop(heartbeat_tx, heartbeat_config, heartbeat_run_count).await;
        });

        // Process incoming messages
        while let Some(result) = inbound.next().await {
            match result {
                Ok(msg) => {
                    self.handle_server_message(msg).await;
                }
                Err(e) => {
                    warn!(error = %e, "Stream error");
                    break;
                }
            }
        }

        // Clean up
        heartbeat_handle.abort();
        self.outbound_tx = None;

        info!("Disconnected from control plane");
        Ok(())
    }

    async fn send_hello(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let info = self.build_worker_info();
        let hello = WorkerHello {
            info: Some(info.into()),
        };

        let msg = RunClientMessage {
            payload: Some(ClientPayload::Hello(hello)),
        };

        if let Some(tx) = &self.outbound_tx {
            tx.send(msg).await?;
        }
        Ok(())
    }

    fn build_worker_info(&self) -> WorkerInfo {
        // Model backend from config
        let backend = ModelBackend::new(&self.config.model_provider, &self.config.model_name)
            .with_context_window(200_000)
            .with_modalities(vec!["text".to_string()]);

        // Agent from config
        let description = get_agent_description(&self.config.agent_name);
        let agent = AgentSpec::new(&self.config.agent_name)
            .with_description(&description)
            .with_backend(backend);

        // Get hostname
        let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());

        WorkerInfo::new(self.config.worker_id.clone(), hostname)
            .with_agent(agent)
            .with_label("env", "development")
    }
}

/// Get the description for a known agent, or a generic description for custom agents.
fn get_agent_description(agent_name: &str) -> String {
    match agent_name {
        "general" => "General-purpose agent that executes any task".to_string(),
        "support_triage" => "Classifies and triages support tickets".to_string(),
        _ => format!("Custom agent: {}", agent_name),
    }
}

impl WorkerConnection {
    async fn handle_server_message(&self, msg: taskrun_proto::pb::RunServerMessage) {
        if let Some(payload) = msg.payload {
            match payload {
                ServerPayload::AssignRun(assignment) => {
                    info!(
                        run_id = %assignment.run_id,
                        task_id = %assignment.task_id,
                        agent = %assignment.agent_name,
                        "Received run assignment"
                    );

                    // Emit JSON event for task assignment
                    json_output::emit_task_assigned(
                        &assignment.run_id,
                        &assignment.task_id,
                        &assignment.agent_name,
                    );

                    // Spawn real execution via Claude Code
                    if let Some(tx) = &self.outbound_tx {
                        let tx = tx.clone();
                        let active_count = self.active_run_count.clone();
                        let executor = self.executor.clone();
                        let sessions = self.sessions.clone();

                        tokio::spawn(async move {
                            execute_real_run(executor, tx, assignment, active_count, sessions)
                                .await;
                        });
                    }
                }
                ServerPayload::CancelRun(cancel) => {
                    info!(
                        run_id = %cancel.run_id,
                        reason = %cancel.reason,
                        "Received cancel request"
                    );

                    // Emit JSON event for task cancellation
                    json_output::emit_task_cancelled(&cancel.run_id, &cancel.reason);

                    // TODO: Cancel the run (would need to track JoinHandles)
                }
                ServerPayload::Ack(ack) => {
                    info!(ack_type = %ack.ack_type, ref_id = %ack.ref_id, "Received ack");
                }
                ServerPayload::ContinueRun(continue_run) => {
                    info!(
                        run_id = %continue_run.run_id,
                        message_len = continue_run.message.len(),
                        "Received continue request"
                    );

                    // Emit JSON event for continue request
                    json_output::emit_continue_received(
                        &continue_run.run_id,
                        continue_run.message.len(),
                    );

                    // Look up session for this run
                    if let Some(tx) = &self.outbound_tx {
                        let tx = tx.clone();
                        let sessions = self.sessions.clone();
                        let executor = self.executor.clone();
                        let active_count = self.active_run_count.clone();

                        tokio::spawn(async move {
                            execute_continue_run(
                                executor,
                                tx,
                                continue_run,
                                sessions,
                                active_count,
                            )
                            .await;
                        });
                    }
                }
            }
        }
    }
}

/// Execute a real run via Claude Code subprocess.
async fn execute_real_run(
    executor: Arc<ClaudeCodeExecutor>,
    tx: mpsc::Sender<RunClientMessage>,
    assignment: RunAssignment,
    active_count: Arc<AtomicU32>,
    sessions: Arc<Mutex<HashMap<String, SessionInfo>>>,
) {
    let run_id = assignment.run_id.clone();
    let task_id = assignment.task_id.clone();

    // Increment active run count
    active_count.fetch_add(1, Ordering::SeqCst);

    info!(run_id = %run_id, agent = %assignment.agent_name, "Starting real execution via Claude Code");

    // Send RUNNING status
    send_status_update(&tx, &run_id, taskrun_proto::pb::RunStatus::Running, None).await;

    // Emit JSON event for task running
    json_output::emit_task_running(&run_id);

    // Create channel for streaming output from executor
    let (chunk_tx, mut chunk_rx) = mpsc::channel::<crate::executor::OutputChunk>(32);

    // Create channel for events from executor
    let (event_tx, mut event_rx) = mpsc::channel::<RunEvent>(32);

    // Spawn event forwarder to send events via gRPC
    let event_tx_grpc = tx.clone();
    let event_handle = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            send_event(&event_tx_grpc, event).await;
        }
    });

    // Spawn executor in background
    let executor_clone = executor.clone();
    let agent_name = assignment.agent_name.clone();
    let input_json = assignment.input_json.clone();
    let run_id_clone = RunId::new(&run_id);
    let task_id_clone = TaskId::new(&task_id);
    let executor_handle = tokio::spawn(async move {
        executor_clone
            .execute(
                &agent_name,
                &input_json,
                chunk_tx,
                event_tx,
                run_id_clone,
                task_id_clone,
            )
            .await
    });

    // Stream chunks as they arrive
    let mut seq = 0u64;
    while let Some(chunk) = chunk_rx.recv().await {
        if !chunk.is_final && !chunk.content.is_empty() {
            // Emit JSON event for output chunk
            json_output::emit_output_chunk(&run_id, seq, &chunk.content, false);
            send_output_chunk(&tx, &run_id, seq, chunk.content, false).await;
            seq += 1;
        }
    }

    // Wait for executor to complete and get result
    let result = executor_handle.await;

    // Wait for event forwarder to finish
    let _ = event_handle.await;

    match result {
        Ok(Ok(exec_result)) => {
            // Store session ID for future continuation
            if let Some(ref session_id) = exec_result.session_id {
                info!(
                    run_id = %run_id,
                    session_id = %session_id,
                    "Storing session for continuation"
                );
                sessions.lock().await.insert(
                    run_id.clone(),
                    SessionInfo {
                        session_id: session_id.clone(),
                        task_id: task_id.clone(),
                    },
                );
            }

            // Send final chunk and emit JSON event
            json_output::emit_output_chunk(&run_id, seq, "", true);
            send_output_chunk(&tx, &run_id, seq, String::new(), true).await;

            // Build the backend info that was used
            let backend_used = taskrun_proto::pb::ModelBackend {
                provider: exec_result.provider.clone(),
                model_name: exec_result.model_used.clone(),
                context_window: 200_000,
                supports_streaming: true,
                modalities: vec!["text".to_string()],
                tools: vec![],
                metadata: HashMap::new(),
            };

            // Send COMPLETED status with backend_used
            send_status_update(
                &tx,
                &run_id,
                taskrun_proto::pb::RunStatus::Completed,
                Some(backend_used),
            )
            .await;

            // Emit JSON event for task completed
            json_output::emit_task_completed(
                &run_id,
                Some(&exec_result.model_used),
                Some(&exec_result.provider),
            );

            info!(run_id = %run_id, "Real execution completed successfully");
        }
        Ok(Err(e)) => {
            // Executor returned an error
            error!(run_id = %run_id, error = %e, "Execution failed");
            send_status_update_with_error(
                &tx,
                &run_id,
                taskrun_proto::pb::RunStatus::Failed,
                e.to_string(),
            )
            .await;

            // Emit JSON event for task failed
            json_output::emit_task_failed(&run_id, &e.to_string());
        }
        Err(e) => {
            // Task panicked or was cancelled
            error!(run_id = %run_id, error = %e, "Executor task failed");
            let error_msg = format!("Executor task failed: {}", e);
            send_status_update_with_error(
                &tx,
                &run_id,
                taskrun_proto::pb::RunStatus::Failed,
                error_msg.clone(),
            )
            .await;

            // Emit JSON event for task failed
            json_output::emit_task_failed(&run_id, &error_msg);
        }
    }

    // Decrement active run count
    active_count.fetch_sub(1, Ordering::SeqCst);
}

/// Execute a continuation of an existing run.
async fn execute_continue_run(
    executor: Arc<ClaudeCodeExecutor>,
    tx: mpsc::Sender<RunClientMessage>,
    continue_run: ContinueRun,
    sessions: Arc<Mutex<HashMap<String, SessionInfo>>>,
    active_count: Arc<AtomicU32>,
) {
    let run_id = continue_run.run_id.clone();
    let message = continue_run.message.clone();

    // Look up session info
    let session_info = {
        let sessions_guard = sessions.lock().await;
        sessions_guard.get(&run_id).cloned()
    };

    let session_info = match session_info {
        Some(info) => info,
        None => {
            warn!(run_id = %run_id, "No session found for continue request");
            return;
        }
    };

    info!(
        run_id = %run_id,
        session_id = %session_info.session_id,
        "Continuing run with existing session"
    );

    // Increment active run count
    active_count.fetch_add(1, Ordering::SeqCst);

    // Send user message as ChatMessage
    send_chat_message(&tx, &run_id, ProtoChatRole::User, message.clone()).await;

    // Send RUNNING status
    send_status_update(&tx, &run_id, taskrun_proto::pb::RunStatus::Running, None).await;

    // Emit JSON event for task running
    json_output::emit_task_running(&run_id);

    // Create channel for streaming output from executor
    let (chunk_tx, mut chunk_rx) = mpsc::channel::<crate::executor::OutputChunk>(32);

    // Create channel for events from executor
    let (event_tx, mut event_rx) = mpsc::channel::<RunEvent>(32);

    // Spawn event forwarder to send events via gRPC
    let event_tx_grpc = tx.clone();
    let event_handle = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            send_event(&event_tx_grpc, event).await;
        }
    });

    // Spawn executor in background with session continuation
    let executor_clone = executor.clone();
    let session_id = session_info.session_id.clone();
    let task_id = session_info.task_id.clone();
    let run_id_clone = RunId::new(&run_id);
    let task_id_clone = TaskId::new(&task_id);
    let executor_handle = tokio::spawn(async move {
        executor_clone
            .execute_follow_up(
                &session_id,
                &message,
                chunk_tx,
                event_tx,
                run_id_clone,
                task_id_clone,
            )
            .await
    });

    // Stream chunks as they arrive
    let mut seq = 0u64;
    let tx_for_chat = tx.clone();
    let run_id_for_chat = run_id.clone();
    let mut full_response = String::new();

    while let Some(chunk) = chunk_rx.recv().await {
        if !chunk.is_final && !chunk.content.is_empty() {
            full_response.push_str(&chunk.content);
            // Emit JSON event for output chunk
            json_output::emit_output_chunk(&run_id, seq, &chunk.content, false);
            send_output_chunk(&tx, &run_id, seq, chunk.content, false).await;
            seq += 1;
        }
    }

    // Wait for executor to complete and get result
    let result = executor_handle.await;

    // Wait for event forwarder to finish
    let _ = event_handle.await;

    match result {
        Ok(Ok(exec_result)) => {
            // Update session ID if it changed
            if let Some(ref new_session_id) = exec_result.session_id {
                sessions.lock().await.insert(
                    run_id.clone(),
                    SessionInfo {
                        session_id: new_session_id.clone(),
                        task_id: session_info.task_id.clone(),
                    },
                );
            }

            // Send assistant response as ChatMessage
            if !full_response.is_empty() {
                send_chat_message(
                    &tx_for_chat,
                    &run_id_for_chat,
                    ProtoChatRole::Assistant,
                    full_response,
                )
                .await;
            }

            // Send final chunk and emit JSON event
            json_output::emit_output_chunk(&run_id, seq, "", true);
            send_output_chunk(&tx, &run_id, seq, String::new(), true).await;

            // Build the backend info that was used
            let backend_used = taskrun_proto::pb::ModelBackend {
                provider: exec_result.provider.clone(),
                model_name: exec_result.model_used.clone(),
                context_window: 200_000,
                supports_streaming: true,
                modalities: vec!["text".to_string()],
                tools: vec![],
                metadata: HashMap::new(),
            };

            // Send COMPLETED status with backend_used
            send_status_update(
                &tx,
                &run_id,
                taskrun_proto::pb::RunStatus::Completed,
                Some(backend_used),
            )
            .await;

            // Emit JSON event for task completed
            json_output::emit_task_completed(
                &run_id,
                Some(&exec_result.model_used),
                Some(&exec_result.provider),
            );

            info!(run_id = %run_id, "Continue execution completed successfully");
        }
        Ok(Err(e)) => {
            error!(run_id = %run_id, error = %e, "Continue execution failed");
            send_status_update_with_error(
                &tx,
                &run_id,
                taskrun_proto::pb::RunStatus::Failed,
                e.to_string(),
            )
            .await;

            // Emit JSON event for task failed
            json_output::emit_task_failed(&run_id, &e.to_string());
        }
        Err(e) => {
            error!(run_id = %run_id, error = %e, "Continue executor task failed");
            let error_msg = format!("Executor task failed: {}", e);
            send_status_update_with_error(
                &tx,
                &run_id,
                taskrun_proto::pb::RunStatus::Failed,
                error_msg.clone(),
            )
            .await;

            // Emit JSON event for task failed
            json_output::emit_task_failed(&run_id, &error_msg);
        }
    }

    // Decrement active run count
    active_count.fetch_sub(1, Ordering::SeqCst);
}

/// Send a status update to the control plane.
async fn send_status_update(
    tx: &mpsc::Sender<RunClientMessage>,
    run_id: &str,
    status: taskrun_proto::pb::RunStatus,
    backend_used: Option<taskrun_proto::pb::ModelBackend>,
) {
    let update = RunStatusUpdate {
        run_id: run_id.to_string(),
        status: status as i32,
        error_message: String::new(),
        backend_used,
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    };

    let msg = RunClientMessage {
        payload: Some(ClientPayload::StatusUpdate(update)),
    };

    if tx.send(msg).await.is_err() {
        warn!(run_id = %run_id, "Failed to send status update");
    }
}

/// Send a status update with an error message to the control plane.
async fn send_status_update_with_error(
    tx: &mpsc::Sender<RunClientMessage>,
    run_id: &str,
    status: taskrun_proto::pb::RunStatus,
    error_message: String,
) {
    let update = RunStatusUpdate {
        run_id: run_id.to_string(),
        status: status as i32,
        error_message,
        backend_used: None,
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    };

    let msg = RunClientMessage {
        payload: Some(ClientPayload::StatusUpdate(update)),
    };

    if tx.send(msg).await.is_err() {
        warn!(run_id = %run_id, "Failed to send status update with error");
    }
}

/// Send an output chunk to the control plane.
async fn send_output_chunk(
    tx: &mpsc::Sender<RunClientMessage>,
    run_id: &str,
    seq: u64,
    content: String,
    is_final: bool,
) {
    let chunk = RunOutputChunk {
        run_id: run_id.to_string(),
        seq,
        content,
        is_final,
        metadata: HashMap::new(),
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    };

    let msg = RunClientMessage {
        payload: Some(ClientPayload::OutputChunk(chunk)),
    };

    if tx.send(msg).await.is_err() {
        warn!(run_id = %run_id, seq = seq, "Failed to send output chunk");
    }
}

/// Send a run event to the control plane.
async fn send_event(tx: &mpsc::Sender<RunClientMessage>, event: RunEvent) {
    use taskrun_core::RunEventType;

    // Convert domain event type to proto event type
    let proto_event_type = match event.event_type {
        RunEventType::ExecutionStarted => taskrun_proto::pb::RunEventType::ExecutionStarted,
        RunEventType::SessionInitialized => taskrun_proto::pb::RunEventType::SessionInitialized,
        RunEventType::ToolRequested => taskrun_proto::pb::RunEventType::ToolRequested,
        RunEventType::ToolCompleted => taskrun_proto::pb::RunEventType::ToolCompleted,
        RunEventType::OutputGenerated => taskrun_proto::pb::RunEventType::OutputGenerated,
        RunEventType::ExecutionCompleted => taskrun_proto::pb::RunEventType::ExecutionCompleted,
        RunEventType::ExecutionFailed => taskrun_proto::pb::RunEventType::ExecutionFailed,
    };

    let proto_event = ProtoRunEvent {
        id: event.id.into_inner(),
        run_id: event.run_id.into_inner(),
        task_id: event.task_id.into_inner(),
        event_type: proto_event_type as i32,
        timestamp_ms: event.timestamp_ms,
        metadata: event.metadata,
    };

    let msg = RunClientMessage {
        payload: Some(ClientPayload::Event(proto_event)),
    };

    if tx.send(msg).await.is_err() {
        warn!("Failed to send event");
    }
}

/// Send a chat message to the control plane.
async fn send_chat_message(
    tx: &mpsc::Sender<RunClientMessage>,
    run_id: &str,
    role: ProtoChatRole,
    content: String,
) {
    let chat_msg = RunChatMessage {
        run_id: run_id.to_string(),
        message: Some(ChatMessage {
            role: role as i32,
            content,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        }),
    };

    let msg = RunClientMessage {
        payload: Some(ClientPayload::ChatMessage(chat_msg)),
    };

    if tx.send(msg).await.is_err() {
        warn!(run_id = %run_id, "Failed to send chat message");
    }
}

async fn run_heartbeat_loop(
    tx: mpsc::Sender<RunClientMessage>,
    config: Arc<Config>,
    active_count: Arc<AtomicU32>,
) {
    let interval = Duration::from_secs(config.heartbeat_interval_secs);
    let mut interval_timer = tokio::time::interval(interval);

    loop {
        interval_timer.tick().await;

        let runs = active_count.load(Ordering::SeqCst);
        let status = if runs > 0 {
            taskrun_proto::pb::WorkerStatus::Busy
        } else {
            taskrun_proto::pb::WorkerStatus::Idle
        };

        let status_str = if runs > 0 { "busy" } else { "idle" };

        // Emit JSON event for heartbeat
        json_output::emit_heartbeat(config.worker_id.as_str(), status_str, runs);

        let heartbeat = WorkerHeartbeat {
            worker_id: config.worker_id.as_str().to_string(),
            status: status as i32,
            active_runs: runs,
            max_concurrent_runs: config.max_concurrent_runs,
            metrics: HashMap::new(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        };

        let msg = RunClientMessage {
            payload: Some(ClientPayload::Heartbeat(heartbeat)),
        };

        if tx.send(msg).await.is_err() {
            // Channel closed, connection lost
            break;
        }
    }
}
