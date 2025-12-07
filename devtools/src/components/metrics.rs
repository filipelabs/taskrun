//! Metrics dashboard component.

use leptos::*;

use crate::api::{fetch_metrics, Metrics as MetricsData};

/// Metrics dashboard with charts.
#[component]
pub fn Metrics() -> impl IntoView {
    let (metrics, set_metrics) = create_signal(MetricsData::default());
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);

    let fetch = move || {
        spawn_local(async move {
            set_loading.set(true);
            match fetch_metrics().await {
                Ok(data) => {
                    set_metrics.set(data);
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

    // Auto-refresh every 5 seconds
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
                <h1 class="text-2xl font-bold">"Metrics"</h1>
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

            // Workers section
            <div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6">
                <div class="bg-gray-800 rounded-lg border border-gray-700 p-6">
                    <h2 class="text-lg font-semibold mb-4">"Workers"</h2>
                    <div class="flex items-end gap-2 h-40">
                        <BarChart
                            value=move || metrics.get().workers_idle
                            max=move || metrics.get().total_workers().max(1)
                            label="Idle"
                            color="bg-green-500"
                        />
                        <BarChart
                            value=move || metrics.get().workers_busy
                            max=move || metrics.get().total_workers().max(1)
                            label="Busy"
                            color="bg-yellow-500"
                        />
                        <BarChart
                            value=move || metrics.get().workers_draining
                            max=move || metrics.get().total_workers().max(1)
                            label="Draining"
                            color="bg-orange-500"
                        />
                        <BarChart
                            value=move || metrics.get().workers_error
                            max=move || metrics.get().total_workers().max(1)
                            label="Error"
                            color="bg-red-500"
                        />
                    </div>
                    <div class="mt-4 text-center text-gray-400">
                        "Total: " {move || metrics.get().total_workers()} " workers"
                    </div>
                </div>

                <div class="bg-gray-800 rounded-lg border border-gray-700 p-6">
                    <h2 class="text-lg font-semibold mb-4">"Tasks"</h2>
                    <div class="flex items-end gap-2 h-40">
                        <BarChart
                            value=move || metrics.get().tasks_pending
                            max=move || metrics.get().total_tasks().max(1)
                            label="Pending"
                            color="bg-gray-500"
                        />
                        <BarChart
                            value=move || metrics.get().tasks_running
                            max=move || metrics.get().total_tasks().max(1)
                            label="Running"
                            color="bg-blue-500"
                        />
                        <BarChart
                            value=move || metrics.get().tasks_completed
                            max=move || metrics.get().total_tasks().max(1)
                            label="Completed"
                            color="bg-green-500"
                        />
                        <BarChart
                            value=move || metrics.get().tasks_failed
                            max=move || metrics.get().total_tasks().max(1)
                            label="Failed"
                            color="bg-red-500"
                        />
                        <BarChart
                            value=move || metrics.get().tasks_cancelled
                            max=move || metrics.get().total_tasks().max(1)
                            label="Cancelled"
                            color="bg-orange-500"
                        />
                    </div>
                    <div class="mt-4 text-center text-gray-400">
                        "Total: " {move || metrics.get().total_tasks()} " tasks"
                    </div>
                </div>
            </div>

            // Raw metrics
            <div class="bg-gray-800 rounded-lg border border-gray-700 p-6">
                <h2 class="text-lg font-semibold mb-4">"Raw Metrics"</h2>
                <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
                    <MetricCard label="Workers Idle" value=move || metrics.get().workers_idle />
                    <MetricCard label="Workers Busy" value=move || metrics.get().workers_busy />
                    <MetricCard label="Workers Draining" value=move || metrics.get().workers_draining />
                    <MetricCard label="Workers Error" value=move || metrics.get().workers_error />
                    <MetricCard label="Tasks Pending" value=move || metrics.get().tasks_pending />
                    <MetricCard label="Tasks Running" value=move || metrics.get().tasks_running />
                    <MetricCard label="Tasks Completed" value=move || metrics.get().tasks_completed />
                    <MetricCard label="Tasks Failed" value=move || metrics.get().tasks_failed />
                </div>
            </div>
        </div>
    }
}

/// Simple bar chart component.
#[component]
fn BarChart<F, M>(
    value: F,
    max: M,
    label: &'static str,
    color: &'static str,
) -> impl IntoView
where
    F: Fn() -> u32 + Copy + 'static,
    M: Fn() -> u32 + Copy + 'static,
{
    view! {
        <div class="flex-1 flex flex-col items-center">
            <div class="flex-1 w-full flex items-end">
                <div
                    class=format!("w-full {} rounded-t transition-all duration-300", color)
                    style:height=move || {
                        let v = value();
                        let m = max();
                        if m == 0 {
                            "0%".to_string()
                        } else {
                            format!("{}%", (v as f32 / m as f32 * 100.0).min(100.0))
                        }
                    }
                />
            </div>
            <div class="mt-2 text-center">
                <div class="text-lg font-bold">{move || value()}</div>
                <div class="text-xs text-gray-500">{label}</div>
            </div>
        </div>
    }
}

/// Metric card component.
#[component]
fn MetricCard<F>(label: &'static str, value: F) -> impl IntoView
where
    F: Fn() -> u32 + Copy + 'static,
{
    view! {
        <div class="bg-gray-700/50 rounded-lg p-4">
            <div class="text-2xl font-bold">{move || value()}</div>
            <div class="text-xs text-gray-400">{label}</div>
        </div>
    }
}
