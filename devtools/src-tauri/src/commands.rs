//! Tauri IPC commands.
//!
//! These commands provide the bridge between the Leptos frontend
//! and the TaskRun control plane via gRPC.

use crate::grpc_client::GrpcClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

// ============================================================================
// Types
// ============================================================================

/// Task representation for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    pub id: String,
    pub agent_name: String,
    pub input_json: String,
    pub status: String,
    pub created_by: String,
    pub created_at: String,
}

/// State type for the gRPC client.
pub type ClientState = Arc<Mutex<Option<GrpcClient>>>;

// ============================================================================
// Helper functions
// ============================================================================

/// Convert proto Task to frontend TaskResponse.
fn task_to_response(task: taskrun_proto::pb::Task) -> TaskResponse {
    let status = match task.status {
        1 => "PENDING",
        2 => "RUNNING",
        3 => "COMPLETED",
        4 => "FAILED",
        5 => "CANCELLED",
        _ => "UNKNOWN",
    };

    // Convert milliseconds timestamp to ISO 8601 string
    let created_at = if task.created_at_ms > 0 {
        chrono::DateTime::from_timestamp_millis(task.created_at_ms)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default()
    } else {
        String::new()
    };

    TaskResponse {
        id: task.id,
        agent_name: task.agent_name,
        input_json: task.input_json,
        status: status.to_string(),
        created_by: task.created_by,
        created_at,
    }
}

// ============================================================================
// Basic commands
// ============================================================================

/// Simple greet command for testing IPC.
#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to TaskRun DevTools.", name)
}

/// Get the control plane URL.
#[tauri::command]
pub fn get_control_plane_url() -> String {
    "http://[::1]:50052".to_string()
}

// ============================================================================
// gRPC connection commands
// ============================================================================

/// Connect to the control plane gRPC server.
#[tauri::command]
pub async fn connect_grpc(client: State<'_, ClientState>) -> Result<bool, String> {
    let mut guard = client.lock().await;
    match GrpcClient::connect().await {
        Ok(c) => {
            *guard = Some(c);
            Ok(true)
        }
        Err(e) => Err(format!("Failed to connect: {}", e)),
    }
}

/// Check if gRPC client is connected.
#[tauri::command]
pub async fn is_grpc_connected(client: State<'_, ClientState>) -> Result<bool, String> {
    let guard = client.lock().await;
    Ok(guard.is_some())
}

// ============================================================================
// Task commands
// ============================================================================

/// List all tasks.
#[tauri::command]
pub async fn list_tasks(client: State<'_, ClientState>) -> Result<Vec<TaskResponse>, String> {
    let mut guard = client.lock().await;
    let grpc_client = guard
        .as_mut()
        .ok_or("gRPC client not connected. Call connect_grpc first.")?;
    let tasks = grpc_client.list_tasks(100).await?;
    Ok(tasks.into_iter().map(task_to_response).collect())
}

/// Create a new task.
#[tauri::command]
pub async fn create_task(
    agent_name: String,
    input_json: String,
    client: State<'_, ClientState>,
) -> Result<TaskResponse, String> {
    let mut guard = client.lock().await;
    let grpc_client = guard
        .as_mut()
        .ok_or("gRPC client not connected. Call connect_grpc first.")?;
    let task = grpc_client.create_task(agent_name, input_json).await?;
    Ok(task_to_response(task))
}

/// Get a task by ID.
#[tauri::command]
pub async fn get_task(id: String, client: State<'_, ClientState>) -> Result<TaskResponse, String> {
    let mut guard = client.lock().await;
    let grpc_client = guard
        .as_mut()
        .ok_or("gRPC client not connected. Call connect_grpc first.")?;
    let task = grpc_client.get_task(id).await?;
    Ok(task_to_response(task))
}

/// Cancel a task by ID.
#[tauri::command]
pub async fn cancel_task(
    id: String,
    client: State<'_, ClientState>,
) -> Result<TaskResponse, String> {
    let mut guard = client.lock().await;
    let grpc_client = guard
        .as_mut()
        .ok_or("gRPC client not connected. Call connect_grpc first.")?;
    let task = grpc_client.cancel_task(id).await?;
    Ok(task_to_response(task))
}
