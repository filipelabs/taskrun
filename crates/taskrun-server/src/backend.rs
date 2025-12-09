//! Backend server thread for the server TUI.
//!
//! Runs gRPC and HTTP servers and forwards events to the UI.

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::mpsc;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tracing::{error, info, warn};

use taskrun_control_plane::crypto::CertificateAuthority;
use taskrun_control_plane::state::{AppState, UiNotification};
use taskrun_control_plane::{http, RunServiceImpl, Scheduler, TaskServiceImpl, WorkerServiceImpl};
use taskrun_core::{Task, TaskId, TaskStatus};

use crate::event::{LogLevel, ServerCommand, ServerUiEvent};

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub grpc_addr: String,
    pub http_addr: String,
    pub tls_cert_path: String,
    pub tls_key_path: String,
    pub ca_cert_path: String,
    pub ca_key_path: String,
    pub worker_cert_validity_days: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            grpc_addr: "[::1]:50051".to_string(),
            http_addr: "[::1]:50052".to_string(),
            tls_cert_path: "certs/server.crt".to_string(),
            tls_key_path: "certs/server.key".to_string(),
            ca_cert_path: "certs/ca.crt".to_string(),
            ca_key_path: "certs/ca.key".to_string(),
            worker_cert_validity_days: 7,
        }
    }
}

/// Run the server backend.
pub async fn run_server_backend(
    config: ServerConfig,
    ui_tx: mpsc::Sender<ServerUiEvent>,
    cmd_rx: mpsc::Receiver<ServerCommand>,
) {
    // Load TLS certificates
    let (_identity, tls_config) = match load_tls(&config, &ui_tx).await {
        Some(tls) => tls,
        None => return,
    };

    // Load CA for certificate signing
    let ca = load_ca(&config, &ui_tx).await;

    // Create shared state with UI notification channel
    let (state, ui_rx) = AppState::with_ui_channel(ca);

    // Clone state for servers
    let state_for_grpc = state.clone();
    let state_for_http = state.clone();
    let state_for_commands = state.clone();

    // Spawn event forwarder
    let ui_tx_clone = ui_tx.clone();
    tokio::spawn(async move {
        forward_notifications(ui_rx, ui_tx_clone).await;
    });

    // Create gRPC services
    let run_service = RunServiceImpl::new(state_for_grpc.clone()).into_server();
    let task_service = TaskServiceImpl::new(state_for_grpc.clone()).into_server();
    let worker_service = WorkerServiceImpl::new(state_for_grpc).into_server();

    // Create HTTP router
    let http_router = http::create_router(state_for_http);

    // Parse addresses
    let grpc_addr: SocketAddr = match config.grpc_addr.parse() {
        Ok(addr) => addr,
        Err(e) => {
            let _ = ui_tx
                .send(ServerUiEvent::ServerError {
                    message: format!("Invalid gRPC address: {}", e),
                })
                .await;
            return;
        }
    };
    let http_addr: SocketAddr = match config.http_addr.parse() {
        Ok(addr) => addr,
        Err(e) => {
            let _ = ui_tx
                .send(ServerUiEvent::ServerError {
                    message: format!("Invalid HTTP address: {}", e),
                })
                .await;
            return;
        }
    };

    // Notify UI that server is starting
    let _ = ui_tx
        .send(ServerUiEvent::ServerStarted {
            grpc_addr: config.grpc_addr.clone(),
            http_addr: config.http_addr.clone(),
        })
        .await;

    log_to_ui(
        &ui_tx,
        LogLevel::Info,
        format!("gRPC server listening on {} (mTLS)", config.grpc_addr),
    )
    .await;
    log_to_ui(
        &ui_tx,
        LogLevel::Info,
        format!("HTTP server listening on {}", config.http_addr),
    )
    .await;

    // Build gRPC server
    let grpc_server = match Server::builder().tls_config(tls_config) {
        Ok(mut builder) => builder
            .add_service(run_service)
            .add_service(task_service)
            .add_service(worker_service)
            .serve(grpc_addr),
        Err(e) => {
            let _ = ui_tx
                .send(ServerUiEvent::ServerError {
                    message: format!("Failed to configure TLS: {}", e),
                })
                .await;
            return;
        }
    };

    // Build HTTP server
    let http_listener = match tokio::net::TcpListener::bind(http_addr).await {
        Ok(listener) => listener,
        Err(e) => {
            let _ = ui_tx
                .send(ServerUiEvent::ServerError {
                    message: format!("Failed to bind HTTP server: {}", e),
                })
                .await;
            return;
        }
    };
    let http_server = axum::serve(http_listener, http_router);

    // Spawn a task to handle commands
    let cmd_ui_tx = ui_tx.clone();
    tokio::spawn(async move {
        handle_commands(cmd_rx, state_for_commands, cmd_ui_tx).await;
    });

    // Run servers concurrently - when one exits or shuts down, the function returns
    tokio::select! {
        result = grpc_server => {
            match result {
                Ok(()) => {
                    info!("gRPC server stopped");
                    log_to_ui(&ui_tx, LogLevel::Info, "gRPC server stopped".to_string()).await;
                }
                Err(e) => {
                    error!(error = %e, "gRPC server error");
                    log_to_ui(&ui_tx, LogLevel::Error, format!("gRPC server error: {}", e)).await;
                }
            }
        }
        result = http_server => {
            match result {
                Ok(()) => {
                    info!("HTTP server stopped");
                    log_to_ui(&ui_tx, LogLevel::Info, "HTTP server stopped".to_string()).await;
                }
                Err(e) => {
                    error!(error = %e, "HTTP server error");
                    log_to_ui(&ui_tx, LogLevel::Error, format!("HTTP server error: {}", e)).await;
                }
            }
        }
    }
}

/// Handle commands from the UI.
async fn handle_commands(
    mut cmd_rx: mpsc::Receiver<ServerCommand>,
    state: Arc<AppState>,
    ui_tx: mpsc::Sender<ServerUiEvent>,
) {
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            ServerCommand::Shutdown => {
                info!("Shutdown requested");
                log_to_ui(&ui_tx, LogLevel::Info, "Shutdown requested".to_string()).await;
                break;
            }
            ServerCommand::CreateTask { agent_name, input_json } => {
                handle_create_task(&state, &ui_tx, agent_name, input_json).await;
            }
            ServerCommand::CancelTask { task_id } => {
                handle_cancel_task(&state, &ui_tx, task_id).await;
            }
            ServerCommand::DisconnectWorker { worker_id } => {
                handle_disconnect_worker(&state, &ui_tx, worker_id).await;
            }
        }
    }
}

async fn load_tls(
    config: &ServerConfig,
    ui_tx: &mpsc::Sender<ServerUiEvent>,
) -> Option<(Identity, ServerTlsConfig)> {
    // Load server certificate and key
    let cert = match std::fs::read(&config.tls_cert_path) {
        Ok(cert) => cert,
        Err(e) => {
            let msg = format!(
                "Failed to read TLS certificate from '{}': {}. Run scripts/gen-dev-certs.sh first.",
                config.tls_cert_path, e
            );
            let _ = ui_tx.send(ServerUiEvent::ServerError { message: msg }).await;
            return None;
        }
    };

    let key = match std::fs::read(&config.tls_key_path) {
        Ok(key) => key,
        Err(e) => {
            let msg = format!(
                "Failed to read TLS key from '{}': {}. Run scripts/gen-dev-certs.sh first.",
                config.tls_key_path, e
            );
            let _ = ui_tx.send(ServerUiEvent::ServerError { message: msg }).await;
            return None;
        }
    };

    let identity = Identity::from_pem(cert, key);

    // Load CA certificate for client verification (mTLS)
    let ca_cert = match std::fs::read(&config.ca_cert_path) {
        Ok(ca) => ca,
        Err(e) => {
            let msg = format!(
                "Failed to read CA certificate from '{}': {}. Run scripts/gen-dev-certs.sh first.",
                config.ca_cert_path, e
            );
            let _ = ui_tx.send(ServerUiEvent::ServerError { message: msg }).await;
            return None;
        }
    };

    let tls_config = ServerTlsConfig::new()
        .identity(identity.clone())
        .client_ca_root(Certificate::from_pem(ca_cert));

    Some((identity, tls_config))
}

async fn load_ca(
    config: &ServerConfig,
    ui_tx: &mpsc::Sender<ServerUiEvent>,
) -> Option<CertificateAuthority> {
    match CertificateAuthority::from_files(
        &config.ca_cert_path,
        &config.ca_key_path,
        config.worker_cert_validity_days as u64,
    ) {
        Ok(ca) => {
            log_to_ui(
                ui_tx,
                LogLevel::Info,
                format!(
                    "Certificate Authority loaded (validity: {} days)",
                    config.worker_cert_validity_days
                ),
            )
            .await;
            Some(ca)
        }
        Err(e) => {
            log_to_ui(
                ui_tx,
                LogLevel::Warn,
                format!("Failed to load CA - enrollment won't work: {}", e),
            )
            .await;
            None
        }
    }
}

async fn forward_notifications(
    mut rx: tokio::sync::broadcast::Receiver<UiNotification>,
    tx: mpsc::Sender<ServerUiEvent>,
) {
    loop {
        match rx.recv().await {
            Ok(notification) => {
                let event = match notification {
                    UiNotification::WorkerConnected {
                        worker_id,
                        hostname,
                        agents,
                    } => ServerUiEvent::WorkerConnected {
                        worker_id,
                        hostname,
                        agents,
                    },
                    UiNotification::WorkerDisconnected { worker_id } => {
                        ServerUiEvent::WorkerDisconnected { worker_id }
                    }
                    UiNotification::WorkerHeartbeat {
                        worker_id,
                        status,
                        active_runs,
                        max_concurrent_runs,
                    } => ServerUiEvent::WorkerHeartbeat {
                        worker_id,
                        status,
                        active_runs,
                        max_concurrent_runs,
                    },
                    UiNotification::TaskCreated { task_id, agent } => {
                        ServerUiEvent::TaskCreated { task_id, agent }
                    }
                    UiNotification::TaskStatusChanged { task_id, status } => {
                        ServerUiEvent::TaskStatusChanged { task_id, status }
                    }
                    UiNotification::RunStatusChanged {
                        run_id,
                        task_id,
                        status,
                        ..
                    } => ServerUiEvent::RunStatusChanged {
                        run_id,
                        task_id,
                        status,
                    },
                    UiNotification::RunOutputChunk {
                        run_id, content, ..
                    } => ServerUiEvent::RunOutputChunk { run_id, content },
                    UiNotification::RunEvent { .. } => {
                        // Skip run events for now - we can add them later if needed
                        continue;
                    }
                };

                if tx.send(event).await.is_err() {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                warn!("UI notification channel lagged by {} messages", n);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }
}

async fn handle_create_task(
    state: &Arc<AppState>,
    ui_tx: &mpsc::Sender<ServerUiEvent>,
    agent_name: String,
    input_json: String,
) {
    // Validate agent exists on a worker
    if !state.has_agent(&agent_name).await {
        log_to_ui(
            ui_tx,
            LogLevel::Error,
            format!("No worker supports agent: {}", agent_name),
        )
        .await;
        return;
    }

    // Create task
    let task = Task::new(&agent_name, &input_json, "server-tui");
    let task_id = task.id.clone();

    log_to_ui(
        ui_tx,
        LogLevel::Info,
        format!("Creating task {} for agent '{}'", task_id, agent_name),
    )
    .await;

    // Store task
    state.tasks.write().await.insert(task_id.clone(), task);

    // Notify UI (the notification from TaskService won't fire since we're bypassing it)
    state.notify_ui(UiNotification::TaskCreated {
        task_id: task_id.clone(),
        agent: agent_name.clone(),
    });

    // Schedule immediately
    let scheduler = Scheduler::new(state.clone());
    match scheduler.assign_task(&task_id).await {
        Ok(run_id) => {
            log_to_ui(
                ui_tx,
                LogLevel::Info,
                format!("Task {} assigned, run {}", task_id, run_id),
            )
            .await;
        }
        Err(e) => {
            log_to_ui(
                ui_tx,
                LogLevel::Warn,
                format!("Task {} created but not assigned: {}", task_id, e),
            )
            .await;
        }
    }
}

async fn handle_cancel_task(
    state: &Arc<AppState>,
    ui_tx: &mpsc::Sender<ServerUiEvent>,
    task_id: TaskId,
) {
    use taskrun_core::RunStatus;
    use taskrun_proto::pb::run_server_message::Payload as ServerPayload;
    use taskrun_proto::pb::{CancelRun, RunServerMessage};

    // Collect runs to cancel
    let runs_to_cancel: Vec<_>;
    {
        let mut tasks = state.tasks.write().await;
        let task = match tasks.get_mut(&task_id) {
            Some(task) => task,
            None => {
                log_to_ui(
                    ui_tx,
                    LogLevel::Error,
                    format!("Task not found: {}", task_id),
                )
                .await;
                return;
            }
        };

        if task.is_terminal() {
            log_to_ui(
                ui_tx,
                LogLevel::Warn,
                format!("Task {} is already terminal: {:?}", task_id, task.status),
            )
            .await;
            return;
        }

        log_to_ui(ui_tx, LogLevel::Info, format!("Cancelling task {}", task_id)).await;

        // Collect active runs
        runs_to_cancel = task
            .runs
            .iter()
            .filter(|r| r.status.is_active())
            .map(|r| (r.worker_id.clone(), r.run_id.clone()))
            .collect();

        // Mark task and runs as cancelled
        task.status = TaskStatus::Cancelled;
        for run in &mut task.runs {
            if run.status.is_active() {
                run.status = RunStatus::Cancelled;
                run.finished_at = Some(chrono::Utc::now());
            }
        }
    }

    // Notify UI
    state.notify_ui(UiNotification::TaskStatusChanged {
        task_id: task_id.clone(),
        status: TaskStatus::Cancelled,
    });

    // Send CancelRun to workers
    if !runs_to_cancel.is_empty() {
        let workers = state.workers.read().await;
        for (worker_id, run_id) in &runs_to_cancel {
            if let Some(worker) = workers.get(worker_id) {
                let cancel_msg = RunServerMessage {
                    payload: Some(ServerPayload::CancelRun(CancelRun {
                        run_id: run_id.to_string(),
                        reason: "Task cancelled by user".to_string(),
                    })),
                };

                if let Err(e) = worker.tx.send(cancel_msg).await {
                    log_to_ui(
                        ui_tx,
                        LogLevel::Warn,
                        format!("Failed to send cancel to worker {}: {}", worker_id, e),
                    )
                    .await;
                }
            }
        }
    }
}

async fn handle_disconnect_worker(
    state: &Arc<AppState>,
    ui_tx: &mpsc::Sender<ServerUiEvent>,
    worker_id: taskrun_core::WorkerId,
) {
    let mut workers = state.workers.write().await;
    if workers.remove(&worker_id).is_some() {
        log_to_ui(
            ui_tx,
            LogLevel::Info,
            format!("Disconnected worker: {}", worker_id),
        )
        .await;

        // Notify UI
        state.notify_ui(UiNotification::WorkerDisconnected {
            worker_id: worker_id.clone(),
        });
    } else {
        log_to_ui(
            ui_tx,
            LogLevel::Warn,
            format!("Worker not found: {}", worker_id),
        )
        .await;
    }
}

async fn log_to_ui(tx: &mpsc::Sender<ServerUiEvent>, level: LogLevel, message: String) {
    let _ = tx.send(ServerUiEvent::LogMessage { level, message }).await;
}
