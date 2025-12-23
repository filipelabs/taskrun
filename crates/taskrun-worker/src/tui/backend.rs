//! Background worker thread for the worker TUI.
//!
//! Manages the connection to the control plane with automatic reconnection.

use std::time::Duration;

use tokio::sync::mpsc;
use tracing::{error, info};

use super::connection::{ConnectionConfig, WorkerConnection};
use super::event::{disconnected_event, WorkerCommand, WorkerUiEvent};
use super::state::{ConnectionState, LogLevel, WorkerConfig};

/// Backoff configuration.
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const BACKOFF_FACTOR: u32 = 2;

/// Calculate the next backoff duration.
fn next_backoff(current: Duration) -> Duration {
    let next = current.saturating_mul(BACKOFF_FACTOR);
    if next > MAX_BACKOFF {
        MAX_BACKOFF
    } else {
        next
    }
}

/// Run the worker backend with automatic reconnection.
///
/// This function runs in a separate thread with its own tokio runtime.
/// It connects to the control plane, handles run assignments, and forwards
/// events to the UI thread via the `ui_tx` channel.
pub async fn run_worker_backend(
    config: WorkerConfig,
    worker_id: String,
    ui_tx: mpsc::Sender<WorkerUiEvent>,
    mut cmd_rx: mpsc::Receiver<WorkerCommand>,
) {
    let mut backoff = INITIAL_BACKOFF;
    let conn_config = ConnectionConfig::from_with_id(&config, worker_id);

    // Log initial configuration
    log_to_ui(
        &ui_tx,
        LogLevel::Info,
        format!(
            "Worker starting: agent={}, model={}/{}",
            config.agent_name, conn_config.model_provider, conn_config.model_name
        ),
    )
    .await;

    // Outer connection loop - keeps trying to connect
    loop {
        // Notify UI we're connecting
        let _ = ui_tx
            .send(WorkerUiEvent::ConnectionStateChanged(
                ConnectionState::Connecting,
            ))
            .await;

        info!(endpoint = %conn_config.control_plane_addr, "Attempting to connect to control plane");
        log_to_ui(
            &ui_tx,
            LogLevel::Info,
            format!(
                "Connecting to control plane at {}",
                conn_config.control_plane_addr
            ),
        )
        .await;

        // Create new connection
        let mut connection = WorkerConnection::new(conn_config.clone(), ui_tx.clone());

        // Try to connect and run (pass cmd_rx for handling ContinueRun commands)
        match connection.connect_and_run(&mut cmd_rx).await {
            Ok(quit_requested) => {
                if quit_requested {
                    info!("Quit requested, shutting down backend");
                    return;
                }
                // Connection closed gracefully (server disconnected)
                info!("Connection closed by server");
                log_to_ui(
                    &ui_tx,
                    LogLevel::Warn,
                    "Connection closed by server".to_string(),
                )
                .await;
            }
            Err(e) => {
                let root_cause = crate::get_root_cause(&*e);
                error!(error = %root_cause, "Connection failed");
                log_to_ui(&ui_tx, LogLevel::Error, root_cause).await;
            }
        }

        // Check if quit was requested (commands may have been consumed by connection)
        if let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                WorkerCommand::Quit => {
                    info!("Received quit command, shutting down backend");
                    return;
                }
                WorkerCommand::ForceReconnect => {
                    // Reset backoff on force reconnect
                    backoff = INITIAL_BACKOFF;
                    continue;
                }
                WorkerCommand::ContinueRun { .. } => {
                    // Ignore - can't continue while not connected
                }
                WorkerCommand::CreateTask { .. } => {
                    // Ignore - can't create task while not connected
                    log_to_ui(
                        &ui_tx,
                        LogLevel::Warn,
                        "Cannot create task: not connected".to_string(),
                    )
                    .await;
                }
            }
        }

        // Notify UI of disconnected state with retry time
        let _ = ui_tx.send(disconnected_event(backoff)).await;

        log_to_ui(
            &ui_tx,
            LogLevel::Info,
            format!(
                "Reconnecting in {}s... (press 'r' to retry now)",
                backoff.as_secs()
            ),
        )
        .await;

        // Wait with backoff, but check for commands (Quit or ForceReconnect)
        if wait_with_commands(&mut cmd_rx, backoff).await {
            // Quit command received
            info!("Received quit command during backoff, shutting down");
            return;
        }

        // Increase backoff for next attempt
        backoff = next_backoff(backoff);
    }
}

/// Wait for the specified duration, but respond to Quit and ForceReconnect commands.
///
/// Returns `true` if Quit was received, `false` if timeout elapsed or ForceReconnect received.
async fn wait_with_commands(
    cmd_rx: &mut mpsc::Receiver<WorkerCommand>,
    duration: Duration,
) -> bool {
    let sleep = tokio::time::sleep(duration);
    tokio::pin!(sleep);

    loop {
        tokio::select! {
            _ = &mut sleep => {
                // Timeout elapsed, continue with reconnect
                return false;
            }
            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    WorkerCommand::Quit => {
                        return true;
                    }
                    WorkerCommand::ForceReconnect => {
                        info!("Force reconnect requested");
                        return false;
                    }
                    WorkerCommand::ContinueRun { .. } => {
                        // Can't continue runs while disconnected, ignore
                        info!("Ignoring ContinueRun command while disconnected");
                    }
                    WorkerCommand::CreateTask { .. } => {
                        // Can't create tasks while disconnected, ignore
                        info!("Ignoring CreateTask command while disconnected");
                    }
                }
            }
        }
    }
}

/// Helper to log a message to the UI.
async fn log_to_ui(ui_tx: &mpsc::Sender<WorkerUiEvent>, level: LogLevel, message: String) {
    let _ = ui_tx
        .send(WorkerUiEvent::LogMessage { level, message })
        .await;
}
