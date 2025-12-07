//! Workers dashboard component.

use leptos::*;

use crate::api::{fetch_workers, WorkerResponse};

/// Workers dashboard showing all connected workers.
#[component]
pub fn Workers() -> impl IntoView {
    let (workers, set_workers) = create_signal::<Vec<WorkerResponse>>(vec![]);
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);

    // Fetch workers on mount and every 5 seconds
    let fetch = move || {
        spawn_local(async move {
            set_loading.set(true);
            match fetch_workers().await {
                Ok(data) => {
                    set_workers.set(data);
                    set_error.set(None);
                }
                Err(e) => {
                    set_error.set(Some(e));
                }
            }
            set_loading.set(false);
        });
    };

    // Initial fetch
    create_effect(move |_| {
        fetch();
    });

    // Auto-refresh
    create_effect(move |_| {
        use gloo_timers::future::TimeoutFuture;
        spawn_local(async move {
            loop {
                TimeoutFuture::new(5000).await;
                fetch();
            }
        });
    });

    view! {
        <div>
            <div class="flex items-center justify-between mb-6">
                <h1 class="text-2xl font-bold">"Workers"</h1>
                <button
                    class="px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded-lg text-sm transition-colors"
                    on:click=move |_| fetch()
                >
                    "Refresh"
                </button>
            </div>

            // Error message
            {move || error.get().map(|e| view! {
                <div class="mb-4 p-4 bg-red-900/50 border border-red-700 rounded-lg text-red-300">
                    {e}
                </div>
            })}

            // Loading state
            {move || loading.get().then(|| view! {
                <div class="text-gray-400">"Loading..."</div>
            })}

            // Workers grid
            <div class="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-4">
                <For
                    each=move || workers.get()
                    key=|w| w.worker_id.clone()
                    children=move |worker| view! { <WorkerCard worker=worker /> }
                />
            </div>

            // Empty state
            {move || (!loading.get() && workers.get().is_empty()).then(|| view! {
                <div class="text-center py-12 text-gray-500">
                    <p class="text-lg">"No workers connected"</p>
                    <p class="text-sm mt-2">"Start a worker to see it here"</p>
                </div>
            })}
        </div>
    }
}

/// Card component for a single worker.
#[component]
fn WorkerCard(worker: WorkerResponse) -> impl IntoView {
    let status_color = match worker.status.as_str() {
        "IDLE" => "bg-green-500",
        "BUSY" => "bg-yellow-500",
        "DRAINING" => "bg-orange-500",
        "ERROR" => "bg-red-500",
        _ => "bg-gray-500",
    };

    view! {
        <div class="bg-gray-800 rounded-lg border border-gray-700 p-4">
            // Header
            <div class="flex items-center justify-between mb-3">
                <div class="flex items-center gap-2">
                    <span class=format!("w-3 h-3 rounded-full {}", status_color)></span>
                    <span class="font-medium">{&worker.hostname}</span>
                </div>
                <span class="text-xs text-gray-500">{&worker.version}</span>
            </div>

            // Worker ID
            <p class="text-xs text-gray-500 font-mono mb-3 truncate" title=worker.worker_id.clone()>
                {&worker.worker_id}
            </p>

            // Stats
            <div class="grid grid-cols-2 gap-2 mb-3 text-sm">
                <div class="bg-gray-700/50 rounded p-2">
                    <div class="text-gray-400 text-xs">"Runs"</div>
                    <div class="font-medium">
                        {worker.active_runs} "/" {worker.max_concurrent_runs}
                    </div>
                </div>
                <div class="bg-gray-700/50 rounded p-2">
                    <div class="text-gray-400 text-xs">"Status"</div>
                    <div class="font-medium">{&worker.status}</div>
                </div>
            </div>

            // Agents
            <div class="border-t border-gray-700 pt-3">
                <div class="text-xs text-gray-400 mb-2">"Agents"</div>
                <div class="space-y-1">
                    <For
                        each=move || worker.agents.clone()
                        key=|a| a.name.clone()
                        children=move |agent| view! {
                            <div class="text-sm">
                                <span class="text-white">{&agent.name}</span>
                                <span class="text-gray-500 text-xs ml-2">
                                    {agent.backends.iter().map(|b| format!("{}/{}", b.provider, b.model_name)).collect::<Vec<_>>().join(", ")}
                                </span>
                            </div>
                        }
                    />
                </div>
            </div>

            // Heartbeat
            <div class="mt-3 text-xs text-gray-500">
                "Last heartbeat: " {&worker.last_heartbeat}
            </div>
        </div>
    }
}
