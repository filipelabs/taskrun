//! Streaming Playground component for testing SSE streaming.

use leptos::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::api::{stream_response, SseEvent, StreamingStatus};

/// Streaming Playground component for testing the /v1/responses SSE endpoint.
#[component]
pub fn Playground() -> impl IntoView {
    let (prompt, set_prompt) = create_signal(String::from("What is 2 + 2? Answer briefly."));
    let (output, set_output) = create_signal(String::new());
    let (status, set_status) = create_signal(StreamingStatus::Idle);
    let (response_id, set_response_id) = create_signal::<Option<String>>(None);
    let (error, set_error) = create_signal::<Option<String>>(None);

    // Run streaming response
    let run_streaming = move |_| {
        let prompt_value = prompt.get();
        if prompt_value.is_empty() {
            return;
        }

        // Reset state
        set_output.set(String::new());
        set_status.set(StreamingStatus::Connecting);
        set_response_id.set(None);
        set_error.set(None);

        // We need to collect events and update state
        // Since we can't pass closures directly, we'll use a channel approach
        // wrapped in Rc<RefCell> to update signals from the callback
        let output_buffer = Rc::new(RefCell::new(String::new()));
        let output_buffer_clone = output_buffer.clone();
        let set_output_clone = set_output.clone();
        let set_status_clone = set_status.clone();
        let set_response_id_clone = set_response_id.clone();
        let set_error_clone = set_error.clone();

        spawn_local(async move {
            let result = stream_response("general", &prompt_value, |event| {
                match event {
                    SseEvent::Created(created) => {
                        set_response_id_clone.set(Some(created.id));
                        set_status_clone.set(StreamingStatus::Streaming);
                    }
                    SseEvent::Delta(delta) => {
                        let mut buffer = output_buffer_clone.borrow_mut();
                        buffer.push_str(&delta.delta.text);
                        set_output_clone.set(buffer.clone());
                    }
                    SseEvent::Completed(_) => {
                        set_status_clone.set(StreamingStatus::Completed);
                    }
                    SseEvent::Failed(failed) => {
                        set_status_clone.set(StreamingStatus::Failed);
                        if let Some(err) = failed.error {
                            set_error_clone.set(Some(err.message));
                        }
                    }
                    SseEvent::Unknown(event_type, _) => {
                        web_sys::console::log_1(&format!("Unknown SSE event: {}", event_type).into());
                    }
                }
            })
            .await;

            if let Err(e) = result {
                set_status.set(StreamingStatus::Failed);
                set_error.set(Some(e));
            }
        });
    };

    let status_display = move || {
        match status.get() {
            StreamingStatus::Idle => ("Idle", "text-gray-400"),
            StreamingStatus::Connecting => ("Connecting...", "text-yellow-400"),
            StreamingStatus::Streaming => ("Streaming...", "text-blue-400"),
            StreamingStatus::Completed => ("Completed", "text-green-400"),
            StreamingStatus::Failed => ("Failed", "text-red-400"),
        }
    };

    view! {
        <div>
            <div class="flex items-center justify-between mb-6">
                <h1 class="text-2xl font-bold">"Streaming Playground"</h1>
                <div class="flex items-center gap-4">
                    <span class=move || status_display().1>
                        {move || status_display().0}
                    </span>
                </div>
            </div>

            // Input section
            <div class="bg-gray-800 rounded-lg border border-gray-700 p-6 mb-6">
                <h2 class="text-lg font-semibold mb-4">"Run Agent (Streaming)"</h2>

                <div class="space-y-4">
                    // Prompt input
                    <div>
                        <label class="block text-sm text-gray-400 mb-1">"Prompt"</label>
                        <textarea
                            class="w-full bg-gray-700 border border-gray-600 rounded-lg px-4 py-2 text-white text-sm h-24 focus:outline-none focus:border-blue-500"
                            placeholder="Enter your prompt..."
                            prop:value=move || prompt.get()
                            on:input=move |ev| set_prompt.set(event_target_value(&ev))
                        />
                    </div>

                    // Run button
                    <button
                        class="px-6 py-2 bg-green-600 hover:bg-green-700 rounded-lg font-medium transition-colors disabled:opacity-50"
                        on:click=run_streaming
                        disabled=move || status.get() == StreamingStatus::Connecting || status.get() == StreamingStatus::Streaming
                    >
                        {move || {
                            match status.get() {
                                StreamingStatus::Connecting => "Connecting...",
                                StreamingStatus::Streaming => "Streaming...",
                                _ => "Run (Stream)",
                            }
                        }}
                    </button>
                </div>
            </div>

            // Error message
            {move || error.get().map(|e| view! {
                <div class="mb-4 p-4 bg-red-900/50 border border-red-700 rounded-lg text-red-300">
                    {e}
                </div>
            })}

            // Output section
            <div class="bg-gray-800 rounded-lg border border-gray-700 p-6">
                <div class="flex items-center justify-between mb-4">
                    <h2 class="text-lg font-semibold">"Output"</h2>
                    {move || response_id.get().map(|id| view! {
                        <span class="text-xs text-gray-500 font-mono">{id}</span>
                    })}
                </div>

                <div class="p-4 bg-gray-900 rounded-lg min-h-[200px] max-h-[500px] overflow-y-auto">
                    {move || {
                        let out = output.get();
                        if out.is_empty() {
                            view! {
                                <p class="text-gray-500 text-sm italic">"Output will appear here as it streams..."</p>
                            }.into_view()
                        } else {
                            view! {
                                <pre class="text-sm text-gray-200 whitespace-pre-wrap font-mono">{out}</pre>
                            }.into_view()
                        }
                    }}
                </div>
            </div>
        </div>
    }
}
