//! TaskRun DevTools - Tauri backend.
//!
//! Provides IPC commands for gRPC communication with the control plane.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod grpc_client;

use std::sync::Arc;
use tokio::sync::Mutex;

fn main() {
    // Initialize gRPC client state (starts as None, connected lazily)
    let client_state: commands::ClientState = Arc::new(Mutex::new(None));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(client_state)
        .invoke_handler(tauri::generate_handler![
            // Basic commands
            commands::greet,
            commands::get_control_plane_url,
            // gRPC connection
            commands::connect_grpc,
            commands::is_grpc_connected,
            // Task operations
            commands::list_tasks,
            commands::create_task,
            commands::get_task,
            commands::cancel_task,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
