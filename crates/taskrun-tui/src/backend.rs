//! Background task for polling the control plane.

use std::time::Duration;

use tokio::sync::mpsc;
use tracing::{debug, error, info};

use taskrun_admin_client::AdminClient;

use crate::event::{BackendCommand, UiEvent};

/// Run the background polling loop.
///
/// This function runs in a separate thread with its own tokio runtime.
/// It connects to the control plane and periodically fetches workers and tasks,
/// sending updates to the UI thread via the `ui_tx` channel.
pub async fn run_backend(
    endpoint: String,
    ca_cert: Option<Vec<u8>>,
    refresh_interval: Duration,
    ui_tx: mpsc::Sender<UiEvent>,
    mut cmd_rx: mpsc::Receiver<BackendCommand>,
) {
    info!(endpoint = %endpoint, "Connecting to control plane");

    // Connect to control plane
    let mut client = match AdminClient::connect(&endpoint, ca_cert.as_deref()).await {
        Ok(c) => {
            info!("Connected to control plane");
            c
        }
        Err(e) => {
            error!(error = %e, "Failed to connect to control plane");
            let _ = ui_tx
                .send(UiEvent::Error(format!("Connection failed: {}", e)))
                .await;
            return;
        }
    };

    // Create tick interval for periodic refresh
    let mut interval = tokio::time::interval(refresh_interval);

    loop {
        tokio::select! {
            // Periodic refresh tick
            _ = interval.tick() => {
                debug!("Refresh tick");

                // Fetch workers
                match client.workers.list().await {
                    Ok(workers) => {
                        debug!(count = workers.len(), "Fetched workers");
                        let _ = ui_tx.send(UiEvent::WorkersUpdated(workers)).await;
                    }
                    Err(e) => {
                        debug!(error = %e, "Failed to fetch workers");
                        let _ = ui_tx.send(UiEvent::Error(format!("Workers: {}", e))).await;
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
                    }
                }
            }

            // Commands from UI thread
            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    BackendCommand::Quit => {
                        info!("Received quit command, shutting down backend");
                        break;
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

    info!("Backend shutdown complete");
}
