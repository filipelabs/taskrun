//! Navigation sidebar component.

use leptos::*;
use leptos_router::*;

/// Navigation sidebar with links to all views.
#[component]
pub fn Sidebar() -> impl IntoView {
    view! {
        <aside class="w-64 bg-gray-800 border-r border-gray-700 flex flex-col">
            // Header
            <div class="p-4 border-b border-gray-700">
                <h1 class="text-xl font-bold text-white">"TaskRun"</h1>
                <p class="text-sm text-gray-400">"DevTools"</p>
            </div>

            // Navigation
            <nav class="flex-1 p-4 space-y-2">
                <NavLink href="/workers" label="Workers" icon="W" />
                <NavLink href="/tasks" label="Tasks" icon="T" />
                <NavLink href="/metrics" label="Metrics" icon="M" />
            </nav>

            // Footer
            <div class="p-4 border-t border-gray-700">
                <ConnectionStatus />
            </div>
        </aside>
    }
}

/// Navigation link component.
#[component]
fn NavLink(href: &'static str, label: &'static str, icon: &'static str) -> impl IntoView {
    view! {
        <A
            href=href
            class="flex items-center gap-3 px-3 py-2 rounded-lg text-gray-300 hover:bg-gray-700 hover:text-white transition-colors"
            active_class="bg-gray-700 text-white"
        >
            <span class="w-8 h-8 flex items-center justify-center bg-gray-600 rounded-lg text-sm font-medium">
                {icon}
            </span>
            <span>{label}</span>
        </A>
    }
}

/// Connection status indicator.
#[component]
fn ConnectionStatus() -> impl IntoView {
    let (connected, set_connected) = create_signal(false);

    // Check connection on mount
    create_effect(move |_| {
        spawn_local(async move {
            match crate::api::fetch_health().await {
                Ok(health) => set_connected.set(health.status == "ok"),
                Err(_) => set_connected.set(false),
            }
        });
    });

    view! {
        <div class="flex items-center gap-2 text-sm">
            <span
                class="w-2 h-2 rounded-full"
                class:bg-green-500=move || connected.get()
                class:bg-red-500=move || !connected.get()
            />
            <span class="text-gray-400">
                {move || if connected.get() { "Connected" } else { "Disconnected" }}
            </span>
        </div>
    }
}
