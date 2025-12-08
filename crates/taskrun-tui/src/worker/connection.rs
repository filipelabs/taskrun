//! Connection management for the worker TUI.
//!
//! Adapted from taskrun-worker to forward events to the UI.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use tracing::{error, info, warn};

use taskrun_core::{AgentSpec, ModelBackend, RunEvent, RunId, TaskId, WorkerId, WorkerInfo};
use taskrun_proto::pb::run_client_message::Payload as ClientPayload;
use taskrun_proto::pb::run_server_message::Payload as ServerPayload;
use taskrun_proto::pb::{
    CreateTaskRequest, RunAssignment, RunClientMessage, RunEvent as ProtoRunEvent, RunOutputChunk,
    RunStatusUpdate, WorkerHeartbeat, WorkerHello,
};
use taskrun_proto::{RunServiceClient, TaskServiceClient};

use super::event::{WorkerCommand, WorkerUiEvent};
use super::executor::ClaudeCodeExecutor;
use super::state::{ConnectionState, LogLevel, WorkerConfig};

/// Internal config used by the connection.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub worker_id: String,
    pub control_plane_addr: String,
    pub tls_ca_cert_path: String,
    pub tls_cert_path: String,
    pub tls_key_path: String,
    pub agent_name: String,
    pub model_provider: String,
    pub model_name: String,
    pub heartbeat_interval_secs: u64,
    pub max_concurrent_runs: u32,
    pub allowed_tools: Option<Vec<String>>,
    pub denied_tools: Option<Vec<String>>,
    pub claude_path: String,
    pub working_dir: String,
}

impl ConnectionConfig {
    /// Create a ConnectionConfig with a pre-generated worker ID.
    pub fn from_with_id(config: &WorkerConfig, worker_id: String) -> Self {
        let (provider, model) = config.parse_model();
        Self {
            worker_id,
            control_plane_addr: config.endpoint.clone(),
            tls_ca_cert_path: config.ca_cert_path.clone(),
            tls_cert_path: config.client_cert_path.clone(),
            tls_key_path: config.client_key_path.clone(),
            agent_name: config.agent_name.clone(),
            model_provider: provider,
            model_name: model,
            heartbeat_interval_secs: 30,
            max_concurrent_runs: config.max_concurrent_runs,
            allowed_tools: config.allowed_tools.clone(),
            denied_tools: config.denied_tools.clone(),
            claude_path: "claude".to_string(),
            working_dir: config.working_dir.clone(),
        }
    }

    /// Generate a new unique worker ID.
    pub fn generate_worker_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

impl From<&WorkerConfig> for ConnectionConfig {
    fn from(config: &WorkerConfig) -> Self {
        Self::from_with_id(config, Self::generate_worker_id())
    }
}

/// Manages connection to the control plane with UI event forwarding.
pub struct WorkerConnection {
    config: Arc<ConnectionConfig>,
    outbound_tx: Option<mpsc::Sender<RunClientMessage>>,
    active_run_count: Arc<AtomicU32>,
    executor: Arc<ClaudeCodeExecutor>,
    ui_tx: mpsc::Sender<WorkerUiEvent>,
    /// Session IDs for each run (for continuation support).
    sessions: HashMap<String, String>,
}

#[allow(dead_code)] // worker_id is for API completeness
impl WorkerConnection {
    /// Create a new WorkerConnection.
    pub fn new(config: ConnectionConfig, ui_tx: mpsc::Sender<WorkerUiEvent>) -> Self {
        let config = Arc::new(config);
        let executor = Arc::new(ClaudeCodeExecutor::new(config.clone()));
        Self {
            config,
            outbound_tx: None,
            active_run_count: Arc::new(AtomicU32::new(0)),
            executor,
            ui_tx,
            sessions: HashMap::new(),
        }
    }

    /// Get the worker ID.
    pub fn worker_id(&self) -> &str {
        &self.config.worker_id
    }

    /// Connect to control plane and run the main loop.
    /// Returns `Ok(true)` if quit was requested, `Ok(false)` on normal disconnect.
    /// Accepts cmd_rx to receive commands from the UI (e.g., ContinueRun).
    pub async fn connect_and_run(
        &mut self,
        cmd_rx: &mut mpsc::Receiver<WorkerCommand>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        self.log(LogLevel::Info, format!("Connecting to control plane at {}", self.config.control_plane_addr));

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
                "Failed to read worker certificate from '{}': {}",
                self.config.tls_cert_path, e
            )
        })?;
        let client_key = std::fs::read(&self.config.tls_key_path).map_err(|e| {
            format!(
                "Failed to read worker key from '{}': {}",
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

        self.log(LogLevel::Info, "Connected to control plane, sending WorkerHello".to_string());

        // Notify UI we're connected
        let _ = self
            .ui_tx
            .send(WorkerUiEvent::ConnectionStateChanged(
                ConnectionState::Connected,
            ))
            .await;

        // Send WorkerHello
        self.send_hello().await?;

        // Start heartbeat task
        let heartbeat_tx = tx.clone();
        let heartbeat_config = self.config.clone();
        let heartbeat_run_count = self.active_run_count.clone();
        let heartbeat_handle = tokio::spawn(async move {
            run_heartbeat_loop(heartbeat_tx, heartbeat_config, heartbeat_run_count).await;
        });

        // Process incoming messages and UI commands
        let mut quit_requested = false;
        loop {
            tokio::select! {
                // Handle server messages
                result = inbound.next() => {
                    match result {
                        Some(Ok(msg)) => {
                            self.handle_server_message(msg).await;
                        }
                        Some(Err(e)) => {
                            self.log(LogLevel::Warn, format!("Stream error: {}", e));
                            break;
                        }
                        None => {
                            // Stream ended
                            break;
                        }
                    }
                }
                // Handle UI commands
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        WorkerCommand::Quit => {
                            info!("Received quit command");
                            quit_requested = true;
                            break;
                        }
                        WorkerCommand::ForceReconnect => {
                            info!("Force reconnect requested");
                            break;
                        }
                        WorkerCommand::ContinueRun { run_id, session_id, message } => {
                            self.handle_continue_run(run_id, session_id, message, tx.clone()).await;
                        }
                        WorkerCommand::CreateTask { prompt } => {
                            self.handle_create_task(prompt).await;
                        }
                    }
                }
            }
        }

        // Clean up
        heartbeat_handle.abort();
        self.outbound_tx = None;

        self.log(LogLevel::Info, "Disconnected from control plane".to_string());
        Ok(quit_requested)
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

    /// Handle a ContinueRun command - resume a session with a follow-up message.
    async fn handle_continue_run(
        &self,
        run_id: String,
        session_id: String,
        message: String,
        _tx: mpsc::Sender<RunClientMessage>,
    ) {
        self.log(
            LogLevel::Info,
            format!("Continuing run {} with session {}", run_id, &session_id[..8.min(session_id.len())]),
        );

        // Notify UI that run is active again (add user message to chat)
        let _ = self.ui_tx
            .send(WorkerUiEvent::RunProgress {
                run_id: run_id.clone(),
                output: String::new(), // Will be populated by streaming
            })
            .await;

        // Create channels for output streaming
        let (output_tx, mut output_rx) = mpsc::channel::<super::executor::OutputChunk>(32);
        let (event_tx, mut event_rx) = mpsc::channel::<RunEvent>(32);

        // Spawn output forwarder to UI
        let ui_tx_clone = self.ui_tx.clone();
        let run_id_clone = run_id.clone();
        let output_handle = tokio::spawn(async move {
            while let Some(chunk) = output_rx.recv().await {
                if !chunk.content.is_empty() {
                    let _ = ui_tx_clone
                        .send(WorkerUiEvent::RunProgress {
                            run_id: run_id_clone.clone(),
                            output: chunk.content,
                        })
                        .await;
                }
            }
        });

        // Spawn event forwarder to UI
        let ui_tx_clone2 = self.ui_tx.clone();
        let run_id_clone2 = run_id.clone();
        let event_handle = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                let _ = ui_tx_clone2
                    .send(WorkerUiEvent::RunEvent {
                        run_id: run_id_clone2.clone(),
                        event_type: format!("{:?}", event.event_type),
                        details: event.metadata.get("tool_name").cloned(),
                    })
                    .await;
            }
        });

        // Execute the follow-up
        let executor = self.executor.clone();
        let run_id_for_exec = RunId::new(&run_id);
        // Use the same task_id (we don't have it, use run_id as placeholder)
        let task_id_for_exec = TaskId::new(&run_id);

        let result = executor
            .execute_follow_up(
                &session_id,
                &message,
                output_tx,
                event_tx,
                run_id_for_exec,
                task_id_for_exec,
            )
            .await;

        // Wait for handlers
        let _ = output_handle.await;
        let _ = event_handle.await;

        match result {
            Ok(exec_result) => {
                // Send new session_id if changed
                if let Some(new_session_id) = &exec_result.session_id {
                    let _ = self.ui_tx
                        .send(WorkerUiEvent::SessionCaptured {
                            run_id: run_id.clone(),
                            session_id: new_session_id.clone(),
                        })
                        .await;
                }

                // Notify UI that turn is complete (finalize output as assistant message)
                let _ = self.ui_tx
                    .send(WorkerUiEvent::TurnCompleted {
                        run_id: run_id.clone(),
                    })
                    .await;

                self.log(LogLevel::Info, format!("Continuation completed for run {}", run_id));
            }
            Err(e) => {
                self.log(LogLevel::Error, format!("Continuation failed: {}", e));
                let _ = self.ui_tx
                    .send(WorkerUiEvent::LogMessage {
                        level: LogLevel::Error,
                        message: format!("Failed to continue session: {}", e),
                    })
                    .await;
            }
        }
    }

    /// Handle a CreateTask command - create a new task via the TaskService API.
    async fn handle_create_task(&self, prompt: String) {
        self.log(LogLevel::Info, format!("Creating new task with prompt: {}", &prompt[..50.min(prompt.len())]));

        // Build JSON input
        let input_json = serde_json::json!({
            "prompt": prompt
        }).to_string();

        // Create the request
        let request = CreateTaskRequest {
            agent_name: self.config.agent_name.clone(),
            input_json,
            labels: std::collections::HashMap::new(),
            created_by: "worker-tui".to_string(),
        };

        // Connect to TaskService (reuse TLS config)
        match self.create_task_client().await {
            Ok(mut client) => {
                match client.create_task(request).await {
                    Ok(response) => {
                        let task = response.into_inner();
                        self.log(
                            LogLevel::Info,
                            format!("Task created: id={}, agent={}", task.id, task.agent_name),
                        );
                    }
                    Err(e) => {
                        self.log(LogLevel::Error, format!("Failed to create task: {}", e));
                    }
                }
            }
            Err(e) => {
                self.log(LogLevel::Error, format!("Failed to connect to TaskService: {}", e));
            }
        }
    }

    /// Create a TaskService client with the same TLS config.
    async fn create_task_client(&self) -> Result<TaskServiceClient<Channel>, Box<dyn std::error::Error + Send + Sync>> {
        let ca_cert = std::fs::read(&self.config.tls_ca_cert_path)?;
        let client_cert = std::fs::read(&self.config.tls_cert_path)?;
        let client_key = std::fs::read(&self.config.tls_key_path)?;

        let tls_config = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(ca_cert))
            .identity(Identity::from_pem(client_cert, client_key))
            .domain_name("localhost");

        let channel = Channel::from_shared(self.config.control_plane_addr.clone())?
            .tls_config(tls_config)?
            .connect()
            .await?;

        Ok(TaskServiceClient::new(channel))
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

        WorkerInfo::new(WorkerId::new(&self.config.worker_id), hostname)
            .with_agent(agent)
            .with_label("env", "development")
    }

    async fn handle_server_message(&self, msg: taskrun_proto::pb::RunServerMessage) {
        if let Some(payload) = msg.payload {
            match payload {
                ServerPayload::AssignRun(assignment) => {
                    self.log(
                        LogLevel::Info,
                        format!(
                            "Received run assignment: run_id={}, agent={}",
                            assignment.run_id, assignment.agent_name
                        ),
                    );

                    // Notify UI of run start
                    let _ = self
                        .ui_tx
                        .send(WorkerUiEvent::RunStarted {
                            run_id: assignment.run_id.clone(),
                            task_id: assignment.task_id.clone(),
                            agent: assignment.agent_name.clone(),
                            input: assignment.input_json.clone(),
                        })
                        .await;

                    // Spawn real execution via Claude Code
                    if let Some(tx) = &self.outbound_tx {
                        let tx = tx.clone();
                        let active_count = self.active_run_count.clone();
                        let executor = self.executor.clone();
                        let ui_tx = self.ui_tx.clone();

                        tokio::spawn(async move {
                            execute_real_run(executor, tx, assignment, active_count, ui_tx).await;
                        });
                    }
                }
                ServerPayload::CancelRun(cancel) => {
                    self.log(
                        LogLevel::Info,
                        format!(
                            "Received cancel request: run_id={}, reason={}",
                            cancel.run_id, cancel.reason
                        ),
                    );
                }
                ServerPayload::Ack(ack) => {
                    self.log(
                        LogLevel::Debug,
                        format!("Received ack: type={}, ref_id={}", ack.ack_type, ack.ref_id),
                    );
                }
            }
        }
    }

    fn log(&self, level: LogLevel, message: String) {
        // Also log via tracing
        match level {
            LogLevel::Debug => tracing::debug!("{}", message),
            LogLevel::Info => tracing::info!("{}", message),
            LogLevel::Warn => tracing::warn!("{}", message),
            LogLevel::Error => tracing::error!("{}", message),
        }

        // Send to UI (non-blocking)
        let _ = self.ui_tx.try_send(WorkerUiEvent::LogMessage { level, message });
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

/// Execute a real run via Claude Code subprocess.
async fn execute_real_run(
    executor: Arc<ClaudeCodeExecutor>,
    tx: mpsc::Sender<RunClientMessage>,
    assignment: RunAssignment,
    active_count: Arc<AtomicU32>,
    ui_tx: mpsc::Sender<WorkerUiEvent>,
) {
    let run_id = assignment.run_id.clone();
    let task_id = assignment.task_id.clone();

    // Increment active run count
    let count = active_count.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = ui_tx.send(WorkerUiEvent::StatsUpdated { active_runs: count }).await;

    info!(run_id = %run_id, agent = %assignment.agent_name, "Starting real execution via Claude Code");

    // Send RUNNING status
    send_status_update(&tx, &run_id, taskrun_proto::pb::RunStatus::Running, None).await;

    // Create channel for streaming output from executor
    let (chunk_tx, mut chunk_rx) = mpsc::channel::<super::executor::OutputChunk>(32);

    // Create channel for events from executor
    let (event_tx, mut event_rx) = mpsc::channel::<RunEvent>(32);

    // Spawn event forwarder to send events via gRPC and UI
    let event_tx_grpc = tx.clone();
    let event_ui_tx = ui_tx.clone();
    let event_run_id = run_id.clone();
    let event_handle = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            // Forward to UI
            let event_type = format!("{:?}", event.event_type);
            let details = event.metadata.get("tool_name").cloned()
                .or_else(|| event.metadata.get("error").cloned());
            let _ = event_ui_tx
                .send(WorkerUiEvent::RunEvent {
                    run_id: event_run_id.clone(),
                    event_type: event_type.clone(),
                    details,
                })
                .await;
            // Forward to gRPC
            send_event(&event_tx_grpc, event).await;
        }
    });

    // Spawn output forwarder to UI
    let ui_tx_output = ui_tx.clone();
    let run_id_output = run_id.clone();
    let output_handle = tokio::spawn(async move {
        let mut seq = 0u64;
        while let Some(chunk) = chunk_rx.recv().await {
            if !chunk.is_final && !chunk.content.is_empty() {
                // Send to UI
                let _ = ui_tx_output
                    .send(WorkerUiEvent::RunProgress {
                        run_id: run_id_output.clone(),
                        output: chunk.content.clone(),
                    })
                    .await;
                seq += 1;
            }
        }
        seq
    });

    // Create separate chunk channel for gRPC streaming
    let (grpc_chunk_tx, mut grpc_chunk_rx) = mpsc::channel::<super::executor::OutputChunk>(32);

    // Spawn gRPC output streamer
    let grpc_tx = tx.clone();
    let run_id_grpc = run_id.clone();
    let grpc_handle = tokio::spawn(async move {
        let mut seq = 0u64;
        while let Some(chunk) = grpc_chunk_rx.recv().await {
            if !chunk.is_final && !chunk.content.is_empty() {
                send_output_chunk(&grpc_tx, &run_id_grpc, seq, chunk.content, false).await;
                seq += 1;
            }
        }
        seq
    });

    // Execute
    let executor_clone = executor.clone();
    let agent_name = assignment.agent_name.clone();
    let input_json = assignment.input_json.clone();
    let run_id_clone = RunId::new(&run_id);
    let task_id_clone = TaskId::new(&task_id);

    // Create a forking sender that sends to both UI and gRPC
    let (fork_tx, mut fork_rx) = mpsc::channel::<super::executor::OutputChunk>(32);
    let chunk_tx_clone = chunk_tx;
    let grpc_chunk_tx_clone = grpc_chunk_tx;
    let fork_handle = tokio::spawn(async move {
        while let Some(chunk) = fork_rx.recv().await {
            let _ = chunk_tx_clone.send(chunk.clone()).await;
            let _ = grpc_chunk_tx_clone.send(chunk).await;
        }
    });

    let result = executor_clone
        .execute(
            &agent_name,
            &input_json,
            fork_tx,
            event_tx,
            run_id_clone,
            task_id_clone,
        )
        .await;

    // Wait for all handlers
    let _ = output_handle.await;
    let seq = grpc_handle.await.unwrap_or(0);
    let _ = event_handle.await;
    let _ = fork_handle.await;

    match result {
        Ok(exec_result) => {
            // Send final chunk
            send_output_chunk(&tx, &run_id, seq, String::new(), true).await;

            // Build the backend info that was used
            let backend_used = taskrun_proto::pb::ModelBackend {
                provider: exec_result.provider,
                model_name: exec_result.model_used,
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

            // Send SessionCaptured event if we have a session_id (enables continuation)
            if let Some(session_id) = &exec_result.session_id {
                let _ = ui_tx
                    .send(WorkerUiEvent::SessionCaptured {
                        run_id: run_id.clone(),
                        session_id: session_id.clone(),
                    })
                    .await;
                info!(run_id = %run_id, session_id = %session_id, "Session ID captured for continuation");
            }

            // Notify UI
            let _ = ui_tx
                .send(WorkerUiEvent::RunCompleted {
                    run_id: run_id.clone(),
                    success: true,
                    error_message: None,
                })
                .await;

            info!(run_id = %run_id, "Real execution completed successfully");
        }
        Err(e) => {
            // Executor returned an error
            error!(run_id = %run_id, error = %e, "Execution failed");
            send_status_update_with_error(
                &tx,
                &run_id,
                taskrun_proto::pb::RunStatus::Failed,
                e.to_string(),
            )
            .await;

            // Notify UI
            let _ = ui_tx
                .send(WorkerUiEvent::RunCompleted {
                    run_id: run_id.clone(),
                    success: false,
                    error_message: Some(e.to_string()),
                })
                .await;
        }
    }

    // Decrement active run count
    let count = active_count.fetch_sub(1, Ordering::SeqCst) - 1;
    let _ = ui_tx.send(WorkerUiEvent::StatsUpdated { active_runs: count }).await;
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

async fn run_heartbeat_loop(
    tx: mpsc::Sender<RunClientMessage>,
    config: Arc<ConnectionConfig>,
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

        let heartbeat = WorkerHeartbeat {
            worker_id: config.worker_id.clone(),
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
