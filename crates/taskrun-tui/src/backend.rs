//! Background task for polling the control plane with automatic reconnection.

use std::time::Duration;

use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use taskrun_admin_client::AdminClient;

use crate::event::{BackendCommand, ConnectionState, UiEvent};

/// Backoff configuration.
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const BACKOFF_FACTOR: u32 = 2;

/// Number of consecutive poll failures before triggering reconnect.
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// Calculate the next backoff duration.
fn next_backoff(current: Duration) -> Duration {
    let next = current.saturating_mul(BACKOFF_FACTOR);
    if next > MAX_BACKOFF {
        MAX_BACKOFF
    } else {
        next
    }
}

/// Run the background polling loop with automatic reconnection.
///
/// This function runs in a separate thread with its own tokio runtime.
/// It connects to the control plane and periodically fetches workers and tasks,
/// sending updates to the UI thread via the `ui_tx` channel.
///
/// On connection failure or persistent poll errors, it will automatically
/// reconnect with exponential backoff.
pub async fn run_backend(
    endpoint: String,
    ca_cert: Option<Vec<u8>>,
    client_identity: Option<(Vec<u8>, Vec<u8>)>,
    refresh_interval: Duration,
    ui_tx: mpsc::Sender<UiEvent>,
    mut cmd_rx: mpsc::Receiver<BackendCommand>,
) {
    let mut backoff = INITIAL_BACKOFF;

    // Outer connection loop - keeps trying to connect
    loop {
        // Notify UI we're connecting
        let _ = ui_tx
            .send(UiEvent::ConnectionStateChanged(ConnectionState::Connecting))
            .await;

        info!(endpoint = %endpoint, "Attempting to connect to control plane");

        // Try to connect
        let client = match AdminClient::connect(&endpoint, ca_cert.as_deref(), client_identity.as_ref()).await {
            Ok(c) => {
                info!("Connected to control plane");
                // Reset backoff on successful connection
                backoff = INITIAL_BACKOFF;
                // Notify UI we're connected
                let _ = ui_tx
                    .send(UiEvent::ConnectionStateChanged(ConnectionState::Connected))
                    .await;
                c
            }
            Err(e) => {
                error!(error = %e, "Failed to connect to control plane");
                let _ = ui_tx
                    .send(UiEvent::Error(format!("Connection failed: {}", e)))
                    .await;

                // Notify UI of disconnected state with retry time
                let _ = ui_tx
                    .send(UiEvent::ConnectionStateChanged(
                        ConnectionState::Disconnected { retry_in: backoff },
                    ))
                    .await;

                // Wait with backoff, but check for commands (Quit or ForceReconnect)
                if wait_with_commands(&mut cmd_rx, backoff).await {
                    // Quit command received
                    info!("Received quit command during backoff, shutting down");
                    return;
                }

                // Increase backoff for next attempt
                backoff = next_backoff(backoff);
                continue;
            }
        };

        // Run the poll loop - returns when disconnected or quit requested
        let should_quit =
            run_poll_loop(client, refresh_interval, &ui_tx, &mut cmd_rx).await;

        if should_quit {
            info!("Backend shutdown complete");
            return;
        }

        // Poll loop exited due to connection issues - reconnect
        warn!("Connection lost, will attempt to reconnect");

        // Notify UI of disconnected state
        let _ = ui_tx
            .send(UiEvent::ConnectionStateChanged(
                ConnectionState::Disconnected { retry_in: backoff },
            ))
            .await;

        // Wait with backoff before reconnecting
        if wait_with_commands(&mut cmd_rx, backoff).await {
            info!("Received quit command during backoff, shutting down");
            return;
        }

        backoff = next_backoff(backoff);
    }
}

/// Wait for the specified duration, but respond to Quit and ForceReconnect commands.
///
/// Returns `true` if Quit was received, `false` if timeout elapsed or ForceReconnect received.
async fn wait_with_commands(
    cmd_rx: &mut mpsc::Receiver<BackendCommand>,
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
                    BackendCommand::Quit => {
                        return true;
                    }
                    BackendCommand::ForceReconnect => {
                        info!("Force reconnect requested");
                        return false;
                    }
                    // Ignore other commands while disconnected
                    _ => {}
                }
            }
        }
    }
}

/// Run the polling loop for an active connection.
///
/// Returns `true` if Quit was requested, `false` if connection was lost.
async fn run_poll_loop(
    mut client: AdminClient,
    refresh_interval: Duration,
    ui_tx: &mpsc::Sender<UiEvent>,
    cmd_rx: &mut mpsc::Receiver<BackendCommand>,
) -> bool {
    let mut interval = tokio::time::interval(refresh_interval);
    let mut consecutive_failures: u32 = 0;

    loop {
        tokio::select! {
            // Periodic refresh tick
            _ = interval.tick() => {
                debug!("Refresh tick");

                let mut had_failure = false;

                // Fetch workers
                match client.workers.list().await {
                    Ok(workers) => {
                        debug!(count = workers.len(), "Fetched workers");
                        let _ = ui_tx.send(UiEvent::WorkersUpdated(workers)).await;
                    }
                    Err(e) => {
                        debug!(error = %e, "Failed to fetch workers");
                        let _ = ui_tx.send(UiEvent::Error(format!("Workers: {}", e))).await;
                        had_failure = true;
                    }
                }

                // Fetch tasks
                match client.tasks.list().await {
                    Ok(tasks) => {
                        debug!(count = tasks.len(), "Fetched tasks");
                        let _ = ui_tx.send(UiEvent::TasksUpdated(tasks)).await;
                    }
                    Err(e) => {
                        debug!(error = %e, "Failed to fetch tasks");
                        let _ = ui_tx.send(UiEvent::Error(format!("Tasks: {}", e))).await;
                        had_failure = true;
                    }
                }

                // Track consecutive failures
                if had_failure {
                    consecutive_failures += 1;
                    warn!(
                        consecutive = consecutive_failures,
                        max = MAX_CONSECUTIVE_FAILURES,
                        "Poll failure"
                    );

                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        warn!("Too many consecutive failures, triggering reconnect");
                        return false; // Signal reconnection needed
                    }
                } else {
                    // Reset on any success
                    if consecutive_failures > 0 {
                        info!("Connection recovered after {} failures", consecutive_failures);
                    }
                    consecutive_failures = 0;
                }
            }

            // Commands from UI thread
            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    BackendCommand::Quit => {
                        info!("Received quit command, shutting down backend");
                        return true;
                    }
                    BackendCommand::ForceReconnect => {
                        info!("Force reconnect requested while connected");
                        // Trigger reconnection
                        return false;
                    }
                    BackendCommand::RefreshWorkers => {
                        debug!("Manual refresh: workers");
                        if let Ok(workers) = client.workers.list().await {
                            let _ = ui_tx.send(UiEvent::WorkersUpdated(workers)).await;
                        }
                    }
                    BackendCommand::RefreshTasks => {
                        debug!("Manual refresh: tasks");
                        if let Ok(tasks) = client.tasks.list().await {
                            let _ = ui_tx.send(UiEvent::TasksUpdated(tasks)).await;
                        }
                    }
                    BackendCommand::SelectTask(_task_id) => {
                        // Will be implemented in future issues for task details
                    }
                    BackendCommand::CancelTask(_task_id) => {
                        // Will be implemented in future issues
                    }
                }
            }
        }
    }
}
