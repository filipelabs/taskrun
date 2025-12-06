//! Connection management for the worker.

use std::collections::HashMap;
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
use taskrun_proto::pb::{RunClientMessage, WorkerHeartbeat, WorkerHello};
use taskrun_proto::RunServiceClient;

use crate::config::Config;

/// Manages connection to the control plane.
pub struct WorkerConnection {
    config: Arc<Config>,
    outbound_tx: Option<mpsc::Sender<RunClientMessage>>,
}

impl WorkerConnection {
    /// Create a new WorkerConnection.
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            outbound_tx: None,
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
        let heartbeat_handle = tokio::spawn(async move {
            run_heartbeat_loop(heartbeat_tx, heartbeat_config).await;
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
        let backend = ModelBackend::new("anthropic", "claude-3-5-sonnet")
            .with_context_window(200_000)
            .with_modalities(vec!["text".to_string()]);

        let agent = AgentSpec::new("echo-agent")
            .with_description("Simple echo agent for testing")
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
                        "Received run assignment (not implemented yet)"
                    );
                    // TODO: Actually execute the run (FIL-61)
                }
                ServerPayload::CancelRun(cancel) => {
                    info!(
                        run_id = %cancel.run_id,
                        reason = %cancel.reason,
                        "Received cancel request"
                    );
                    // TODO: Cancel the run
                }
                ServerPayload::Ack(ack) => {
                    info!(ack_type = %ack.ack_type, ref_id = %ack.ref_id, "Received ack");
                }
            }
        }
    }
}

async fn run_heartbeat_loop(tx: mpsc::Sender<RunClientMessage>, config: Arc<Config>) {
    let interval = Duration::from_secs(config.heartbeat_interval_secs);
    let mut interval_timer = tokio::time::interval(interval);

    loop {
        interval_timer.tick().await;

        let heartbeat = WorkerHeartbeat {
            worker_id: config.worker_id.as_str().to_string(),
            status: taskrun_proto::pb::WorkerStatus::Idle as i32,
            active_runs: 0, // TODO: Track actual active runs
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
