//! Connection management for the worker.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::transport::Channel;
use tracing::{info, warn};

use taskrun_core::{AgentSpec, ModelBackend, WorkerInfo};
use taskrun_proto::pb::run_client_message::Payload as ClientPayload;
use taskrun_proto::pb::run_server_message::Payload as ServerPayload;
use taskrun_proto::pb::{
    RunClientMessage, RunOutputChunk, RunStatusUpdate, WorkerHeartbeat, WorkerHello,
};
use taskrun_proto::RunServiceClient;

use crate::config::Config;

/// Manages connection to the control plane.
pub struct WorkerConnection {
    config: Arc<Config>,
    outbound_tx: Option<mpsc::Sender<RunClientMessage>>,
    active_run_count: Arc<AtomicU32>,
}

impl WorkerConnection {
    /// Create a new WorkerConnection.
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            outbound_tx: None,
            active_run_count: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Connect to control plane and run the main loop.
    /// Returns on disconnect (caller should handle reconnection).
    pub async fn connect_and_run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(addr = %self.config.control_plane_addr, "Connecting to control plane");

        let channel = Channel::from_shared(self.config.control_plane_addr.clone())?
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

                    // Spawn fake execution
                    if let Some(tx) = &self.outbound_tx {
                        let tx = tx.clone();
                        let run_id = assignment.run_id.clone();
                        let active_count = self.active_run_count.clone();

                        tokio::spawn(async move {
                            execute_fake_run(tx, run_id, active_count).await;
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

/// Execute a fake run - simulates agent execution with delays.
async fn execute_fake_run(
    tx: mpsc::Sender<RunClientMessage>,
    run_id: String,
    active_count: Arc<AtomicU32>,
) {
    // Increment active run count
    active_count.fetch_add(1, Ordering::SeqCst);

    info!(run_id = %run_id, "Starting fake execution");

    // Send RUNNING status (no backend yet)
    send_status_update(&tx, &run_id, taskrun_proto::pb::RunStatus::Running, None).await;

    // Simulate work with fake output chunks
    for i in 0..3 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let content = format!("Processing step {}... This is simulated output from the agent.", i + 1);
        send_output_chunk(&tx, &run_id, i, content, false).await;
    }

    // Send final chunk
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_output_chunk(&tx, &run_id, 3, "Execution completed successfully.".to_string(), true).await;

    // Build the backend info that was used
    let backend_used = taskrun_proto::pb::ModelBackend {
        provider: "anthropic".to_string(),
        model_name: "claude-opus-4-5".to_string(),
        context_window: 200_000,
        supports_streaming: true,
        modalities: vec!["text".to_string()],
        tools: vec![],
        metadata: std::collections::HashMap::new(),
    };

    // Send COMPLETED status with backend_used
    send_status_update(&tx, &run_id, taskrun_proto::pb::RunStatus::Completed, Some(backend_used)).await;

    // Decrement active run count
    active_count.fetch_sub(1, Ordering::SeqCst);

    info!(run_id = %run_id, "Fake execution completed");
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
