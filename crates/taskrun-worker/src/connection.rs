//! Connection management for the worker.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use tracing::{error, info, warn};

use taskrun_core::{AgentSpec, ModelBackend, WorkerInfo};
use taskrun_proto::pb::run_client_message::Payload as ClientPayload;
use taskrun_proto::pb::run_server_message::Payload as ServerPayload;
use taskrun_proto::pb::{
    RunAssignment, RunClientMessage, RunOutputChunk, RunStatusUpdate, WorkerHeartbeat, WorkerHello,
};
use taskrun_proto::RunServiceClient;

use crate::config::Config;
use crate::executor::ClaudeCodeExecutor;

/// Manages connection to the control plane.
pub struct WorkerConnection {
    config: Arc<Config>,
    outbound_tx: Option<mpsc::Sender<RunClientMessage>>,
    active_run_count: Arc<AtomicU32>,
    executor: Arc<ClaudeCodeExecutor>,
}

impl WorkerConnection {
    /// Create a new WorkerConnection.
    pub fn new(config: Arc<Config>) -> Self {
        let executor = Arc::new(ClaudeCodeExecutor::new(config.claude_path.clone()));
        Self {
            config,
            outbound_tx: None,
            active_run_count: Arc::new(AtomicU32::new(0)),
            executor,
        }
    }

    /// Connect to control plane and run the main loop.
    /// Returns on disconnect (caller should handle reconnection).
    pub async fn connect_and_run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        // Hardcoded agent for now
        let backend = ModelBackend::new("anthropic", "claude-opus-4-5")
            .with_context_window(200_000)
            .with_modalities(vec!["text".to_string()]);

        let agent = AgentSpec::new("support_triage")
            .with_description("Support ticket triage agent")
            .with_backend(backend);

        // Get hostname
        let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());

        WorkerInfo::new(self.config.worker_id.clone(), hostname)
            .with_agent(agent)
            .with_label("env", "development")
    }

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

                    // Spawn real execution via Claude Code
                    if let Some(tx) = &self.outbound_tx {
                        let tx = tx.clone();
                        let active_count = self.active_run_count.clone();
                        let executor = self.executor.clone();

                        tokio::spawn(async move {
                            execute_real_run(executor, tx, assignment, active_count).await;
                        });
                    }
                }
                ServerPayload::CancelRun(cancel) => {
                    info!(
                        run_id = %cancel.run_id,
                        reason = %cancel.reason,
                        "Received cancel request"
                    );
                    // TODO: Cancel the run (would need to track JoinHandles)
                }
                ServerPayload::Ack(ack) => {
                    info!(ack_type = %ack.ack_type, ref_id = %ack.ref_id, "Received ack");
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
) {
    let run_id = assignment.run_id.clone();

    // Increment active run count
    active_count.fetch_add(1, Ordering::SeqCst);

    info!(run_id = %run_id, agent = %assignment.agent_name, "Starting real execution via Claude Code");

    // Send RUNNING status
    send_status_update(&tx, &run_id, taskrun_proto::pb::RunStatus::Running, None).await;

    // Create channel for streaming output from executor
    let (chunk_tx, mut chunk_rx) = mpsc::channel::<crate::executor::OutputChunk>(32);

    // Spawn executor in background
    let executor_clone = executor.clone();
    let agent_name = assignment.agent_name.clone();
    let input_json = assignment.input_json.clone();
    let executor_handle = tokio::spawn(async move {
        executor_clone.execute(&agent_name, &input_json, chunk_tx).await
    });

    // Stream chunks as they arrive
    let mut seq = 0u64;
    while let Some(chunk) = chunk_rx.recv().await {
        if !chunk.is_final && !chunk.content.is_empty() {
            send_output_chunk(&tx, &run_id, seq, chunk.content, false).await;
            seq += 1;
        }
    }

    // Wait for executor to complete and get result
    let result = executor_handle.await;

    match result {
        Ok(Ok(exec_result)) => {
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
        }
        Err(e) => {
            // Task panicked or was cancelled
            error!(run_id = %run_id, error = %e, "Executor task failed");
            send_status_update_with_error(
                &tx,
                &run_id,
                taskrun_proto::pb::RunStatus::Failed,
                format!("Executor task failed: {}", e),
            )
            .await;
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
