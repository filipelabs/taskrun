//! Tasks view component.
//!
//! Manages tasks via Tauri IPC commands that communicate with the
//! control plane using gRPC.

use leptos::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// ============================================================================
// Types
// ============================================================================

/// Task response from the Tauri backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    pub id: String,
    pub agent_name: String,
    pub input_json: String,
    pub status: String,
    pub created_by: String,
    pub created_at: String,
}

// ============================================================================
// Tauri IPC bindings
// ============================================================================

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Connect to the gRPC server.
async fn call_connect_grpc() -> Result<bool, String> {
    let result = invoke("connect_grpc", JsValue::NULL)
        .await
        .map_err(|e| format!("{:?}", e))?;
    serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
}

/// List all tasks.
async fn call_list_tasks() -> Result<Vec<TaskResponse>, String> {
    let result = invoke("list_tasks", JsValue::NULL)
        .await
        .map_err(|e| format!("{:?}", e))?;
    serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
}

/// Create a new task.
async fn call_create_task(agent_name: &str, input_json: &str) -> Result<TaskResponse, String> {
    let args = serde_wasm_bindgen::to_value(&serde_json::json!({
        "agentName": agent_name,
        "inputJson": input_json,
    }))
    .map_err(|e| e.to_string())?;

    let result = invoke("create_task", args)
        .await
        .map_err(|e| format!("{:?}", e))?;
    serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
}

/// Cancel a task by ID.
async fn call_cancel_task(id: &str) -> Result<TaskResponse, String> {
    let args = serde_wasm_bindgen::to_value(&serde_json::json!({
        "id": id,
    }))
    .map_err(|e| e.to_string())?;

    let result = invoke("cancel_task", args)
        .await
        .map_err(|e| format!("{:?}", e))?;
    serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
}

// ============================================================================
// Components
// ============================================================================

/// Tasks view for managing tasks.
#[component]
pub fn Tasks() -> impl IntoView {
    let (tasks, set_tasks) = create_signal(Vec::<TaskResponse>::new());
    let (loading, set_loading) = create_signal(false);
    let (connected, set_connected) = create_signal(false);
    let (error, set_error) = create_signal::<Option<String>>(None);
    let (agent_name, set_agent_name) = create_signal(String::from("support_triage"));
    let (input_json, set_input_json) = create_signal(String::from(
        r#"{"ticket_id": "TEST-1", "subject": "Test ticket", "body": "This is a test."}"#,
    ));

    // Connect to gRPC on mount
    let connect = move || {
        spawn_local(async move {
            set_loading.set(true);
            set_error.set(None);

            match call_connect_grpc().await {
                Ok(_) => {
                    set_connected.set(true);
                    // Fetch tasks after connecting
                    match call_list_tasks().await {
                        Ok(task_list) => set_tasks.set(task_list),
                        Err(e) => set_error.set(Some(format!("Failed to list tasks: {}", e))),
                    }
                }
                Err(e) => {
                    set_error.set(Some(format!("Failed to connect: {}", e)));
                }
            }
            set_loading.set(false);
        });
    };

    // Auto-connect on mount
    create_effect(move |_| {
        connect();
    });

    // Refresh tasks
    let refresh = move |_| {
        if !connected.get() {
            return;
        }
        spawn_local(async move {
            set_loading.set(true);
            match call_list_tasks().await {
                Ok(task_list) => {
                    set_tasks.set(task_list);
                    set_error.set(None);
                }
                Err(e) => set_error.set(Some(format!("Failed to list tasks: {}", e))),
            }
            set_loading.set(false);
        });
    };

    // Create task handler
    let create_task = move |_| {
        let agent = agent_name.get();
        let input = input_json.get();

        spawn_local(async move {
            set_loading.set(true);
            match call_create_task(&agent, &input).await {
                Ok(_task) => {
                    set_error.set(None);
                    // Refresh task list
                    if let Ok(task_list) = call_list_tasks().await {
                        set_tasks.set(task_list);
                    }
                }
                Err(e) => set_error.set(Some(format!("Failed to create task: {}", e))),
            }
            set_loading.set(false);
        });
    };

    view! {
        <div>
            <div class="flex items-center justify-between mb-6">
                <h1 class="text-2xl font-bold">"Tasks"</h1>
                <div class="flex items-center gap-4">
                    // Connection status
                    <span class=move || {
                        if connected.get() {
                            "text-green-400 text-sm"
                        } else {
                            "text-red-400 text-sm"
                        }
                    }>
                        {move || if connected.get() { "Connected" } else { "Disconnected" }}
                    </span>
                    // Refresh button
                    <button
                        class="px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded-lg text-sm transition-colors disabled:opacity-50"
                        on:click=refresh
                        disabled=move || !connected.get() || loading.get()
                    >
                        {move || if loading.get() { "Loading..." } else { "Refresh" }}
                    </button>
                </div>
            </div>

            // Error message
            {move || error.get().map(|e| view! {
                <div class="mb-4 p-4 bg-red-900/50 border border-red-700 rounded-lg text-red-300">
                    {e}
                </div>
            })}

            // Create task form
            <div class="bg-gray-800 rounded-lg border border-gray-700 p-6 mb-6">
                <h2 class="text-lg font-semibold mb-4">"Create Task"</h2>

                <div class="space-y-4">
                    // Agent name
                    <div>
                        <label class="block text-sm text-gray-400 mb-1">"Agent Name"</label>
                        <input
                            type="text"
                            class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2 text-white focus:outline-none focus:border-blue-500"
                            prop:value=move || agent_name.get()
                            on:input=move |ev| set_agent_name.set(event_target_value(&ev))
                        />
                    </div>

                    // Input JSON
                    <div>
                        <label class="block text-sm text-gray-400 mb-1">"Input JSON"</label>
                        <textarea
                            class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2 text-white font-mono text-sm h-32 focus:outline-none focus:border-blue-500"
                            prop:value=move || input_json.get()
                            on:input=move |ev| set_input_json.set(event_target_value(&ev))
                        />
                    </div>

                    // Submit button
                    <button
                        class="px-6 py-2 bg-green-600 hover:bg-green-700 rounded-lg font-medium transition-colors disabled:opacity-50"
                        on:click=create_task
                        disabled=move || !connected.get() || loading.get()
                    >
                        "Create Task"
                    </button>
                </div>
            </div>

            // Task list
            <div class="bg-gray-800 rounded-lg border border-gray-700 p-6">
                <h2 class="text-lg font-semibold mb-4">"Task History"</h2>

                {move || {
                    let task_list = tasks.get();
                    if task_list.is_empty() {
                        view! {
                            <p class="text-gray-500 text-sm">
                                "No tasks yet. Create one above!"
                            </p>
                        }.into_view()
                    } else {
                        view! {
                            <div class="space-y-3">
                                <For
                                    each=move || tasks.get()
                                    key=|task| task.id.clone()
                                    children=move |task| view! { <TaskCard task=task /> }
                                />
                            </div>
                        }.into_view()
                    }
                }}
            </div>
        </div>
    }
}

/// Card displaying a single task.
#[component]
fn TaskCard(task: TaskResponse) -> impl IntoView {
    let status_color = match task.status.as_str() {
        "PENDING" => "bg-gray-500",
        "RUNNING" => "bg-blue-500",
        "COMPLETED" => "bg-green-500",
        "FAILED" => "bg-red-500",
        "CANCELLED" => "bg-orange-500",
        _ => "bg-gray-500",
    };

    let task_id = task.id.clone();
    let on_cancel = move |_| {
        let id = task_id.clone();
        spawn_local(async move {
            let _ = call_cancel_task(&id).await;
        });
    };

    view! {
        <div class="bg-gray-700/50 rounded-lg p-4">
            <div class="flex justify-between items-start">
                <div class="flex-1">
                    <div class="flex items-center gap-2">
                        <span class=format!("px-2 py-0.5 rounded text-xs font-medium {}", status_color)>
                            {&task.status}
                        </span>
                        <span class="font-semibold">{&task.agent_name}</span>
                    </div>
                    <div class="mt-1 font-mono text-xs text-gray-400 truncate">
                        {&task.id}
                    </div>
                    <div class="mt-2 text-sm text-gray-400">
                        "Created: " {&task.created_at}
                    </div>
                </div>

                // Cancel button (only for pending/running tasks)
                {(task.status == "PENDING" || task.status == "RUNNING").then(|| view! {
                    <button
                        class="px-3 py-1 bg-red-600 hover:bg-red-700 rounded text-sm transition-colors"
                        on:click=on_cancel
                    >
                        "Cancel"
                    </button>
                })}
            </div>

            // Input JSON preview
            <div class="mt-3 p-2 bg-gray-900/50 rounded text-xs font-mono text-gray-400 overflow-x-auto">
                {&task.input_json}
            </div>
        </div>
    }
}
