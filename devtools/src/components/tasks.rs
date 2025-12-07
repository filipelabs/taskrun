//! Tasks view component.
//!
//! Manages tasks via Tauri IPC commands that communicate with the
//! control plane using gRPC.

use leptos::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::api::{fetch_task_events, fetch_task_output, EventResponse};

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
    let (agent_name, _set_agent_name) = create_signal(String::from("general"));
    let (task_prompt, set_task_prompt) =
        create_signal(String::from("What is 2 + 2? Answer briefly."));

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
        let task = task_prompt.get();
        // Wrap the task in JSON format for the general agent
        let input_json = serde_json::json!({"task": task}).to_string();

        spawn_local(async move {
            set_loading.set(true);
            match call_create_task(&agent, &input_json).await {
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
                    // Task prompt
                    <div>
                        <label class="block text-sm text-gray-400 mb-1">"Task"</label>
                        <textarea
                            class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2 text-white text-sm h-24 focus:outline-none focus:border-blue-500"
                            placeholder="Describe what you want Claude to do..."
                            prop:value=move || task_prompt.get()
                            on:input=move |ev| set_task_prompt.set(event_target_value(&ev))
                        />
                    </div>

                    // Submit button
                    <button
                        class="px-6 py-2 bg-green-600 hover:bg-green-700 rounded-lg font-medium transition-colors disabled:opacity-50"
                        on:click=create_task
                        disabled=move || !connected.get() || loading.get()
                    >
                        "Run Task"
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

/// Card displaying a single task with expandable event timeline.
#[component]
fn TaskCard(task: TaskResponse) -> impl IntoView {
    let (expanded, set_expanded) = create_signal(false);
    let (events, set_events) = create_signal(Vec::<EventResponse>::new());
    let (output, set_output) = create_signal::<Option<String>>(None);
    let (loading_events, set_loading_events) = create_signal(false);

    let status_color = match task.status.as_str() {
        "PENDING" => "bg-gray-500",
        "RUNNING" => "bg-blue-500",
        "COMPLETED" => "bg-green-500",
        "FAILED" => "bg-red-500",
        "CANCELLED" => "bg-orange-500",
        _ => "bg-gray-500",
    };

    let task_id_for_cancel = task.id.clone();
    let on_cancel = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation(); // Prevent card expansion
        let id = task_id_for_cancel.clone();
        spawn_local(async move {
            let _ = call_cancel_task(&id).await;
        });
    };

    let task_id_for_expand = task.id.clone();
    let task_id_for_output = task.id.clone();
    let on_toggle = move |_| {
        let new_expanded = !expanded.get();
        set_expanded.set(new_expanded);

        // Fetch events and output when expanding (if not already loaded)
        if new_expanded && events.get().is_empty() {
            let id = task_id_for_expand.clone();
            let id_for_output = task_id_for_output.clone();
            spawn_local(async move {
                set_loading_events.set(true);
                // Fetch events
                if let Ok(evt_list) = fetch_task_events(&id).await {
                    set_events.set(evt_list);
                }
                // Fetch output
                if let Ok(out_resp) = fetch_task_output(&id_for_output).await {
                    set_output.set(out_resp.output);
                }
                set_loading_events.set(false);
            });
        }
    };

    let can_cancel = task.status == "PENDING" || task.status == "RUNNING";
    let task_id_display = task.id.clone();
    let agent_name = task.agent_name.clone();
    let created_at = task.created_at.clone();
    let input_json = task.input_json.clone();

    view! {
        <div class="bg-gray-700/50 rounded-lg overflow-hidden">
            // Clickable header
            <div
                class="p-4 cursor-pointer hover:bg-gray-700/70 transition-colors"
                on:click=on_toggle
            >
                <div class="flex justify-between items-start">
                    <div class="flex-1">
                        <div class="flex items-center gap-2">
                            // Expand indicator
                            <span class="text-gray-400 text-xs">
                                {move || if expanded.get() { "▼" } else { "▶" }}
                            </span>
                            <span class=format!("px-2 py-0.5 rounded text-xs font-medium {}", status_color)>
                                {&task.status}
                            </span>
                            <span class="font-semibold">{agent_name.clone()}</span>
                        </div>
                        <div class="mt-1 font-mono text-xs text-gray-400 truncate ml-5">
                            {task_id_display.clone()}
                        </div>
                        <div class="mt-2 text-sm text-gray-400 ml-5">
                            "Created: " {created_at.clone()}
                        </div>
                    </div>

                    // Cancel button (only for pending/running tasks)
                    {can_cancel.then(|| view! {
                        <button
                            class="px-3 py-1 bg-red-600 hover:bg-red-700 rounded text-sm transition-colors"
                            on:click=on_cancel
                        >
                            "Cancel"
                        </button>
                    })}
                </div>

                // Input JSON preview (always visible)
                <div class="mt-3 ml-5 p-2 bg-gray-900/50 rounded text-xs font-mono text-gray-400 overflow-x-auto">
                    {input_json.clone()}
                </div>
            </div>

            // Expandable event timeline section
            {move || expanded.get().then(|| view! {
                <div class="border-t border-gray-600 p-4 bg-gray-800/50">
                    <h4 class="text-sm font-semibold text-gray-300 mb-3">"Execution Timeline"</h4>

                    {move || {
                        if loading_events.get() {
                            view! {
                                <div class="text-sm text-gray-400">"Loading..."</div>
                            }.into_view()
                        } else {
                            let event_list = events.get();
                            if event_list.is_empty() {
                                view! {
                                    <div class="text-sm text-gray-500">"No events recorded"</div>
                                }.into_view()
                            } else {
                                view! {
                                    <div class="space-y-2">
                                        <For
                                            each=move || events.get()
                                            key=|e| e.id.clone()
                                            children=move |event| view! { <EventRow event=event /> }
                                        />
                                    </div>
                                }.into_view()
                            }
                        }
                    }}

                    // Output section
                    {move || output.get().map(|out| view! {
                        <div class="mt-6 pt-4 border-t border-gray-600">
                            <h4 class="text-sm font-semibold text-gray-300 mb-3">"Output"</h4>
                            <div class="p-3 bg-gray-900 rounded text-sm font-mono text-gray-200 whitespace-pre-wrap overflow-x-auto max-h-96 overflow-y-auto">
                                {out}
                            </div>
                        </div>
                    })}
                </div>
            })}
        </div>
    }
}

/// A single event row in the timeline.
#[component]
fn EventRow(event: EventResponse) -> impl IntoView {
    let event_type_display = event.event_type_display().to_string();
    let event_icon = event.event_icon().to_string();
    let event_color = event.event_color().to_string();

    // Format timestamp
    let timestamp = chrono::DateTime::from_timestamp_millis(event.timestamp_ms)
        .map(|dt| dt.format("%H:%M:%S%.3f").to_string())
        .unwrap_or_else(|| format!("{}ms", event.timestamp_ms));

    // Build metadata display
    let metadata_items: Vec<(String, String)> = event
        .metadata
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    view! {
        <div class="flex items-start gap-3 text-sm">
            // Timeline dot
            <div class="flex-shrink-0 pt-0.5">
                <span class=format!("{}", event_color)>{event_icon}</span>
            </div>

            // Event content
            <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                    <span class="font-medium text-gray-200">{event_type_display}</span>
                    <span class="text-xs text-gray-500 font-mono">{timestamp}</span>
                </div>

                // Metadata (if any)
                {(!metadata_items.is_empty()).then(|| view! {
                    <div class="mt-1 text-xs text-gray-400 space-x-2">
                        {metadata_items.iter().map(|(k, v)| {
                            let key = k.clone();
                            let value = v.clone();
                            view! {
                                <span>
                                    <span class="text-gray-500">{key}":"</span>
                                    " "
                                    <span class="font-mono">{value}</span>
                                </span>
                            }
                        }).collect_view()}
                    </div>
                })}
            </div>
        </div>
    }
}
