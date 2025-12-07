//! SSE streaming client for /v1/responses endpoint.
//!
//! Uses fetch API with ReadableStream since the endpoint requires POST requests.

use js_sys::{Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, RequestMode, Response};

use super::types::SseEvent;

/// Base URL for the control plane HTTP API.
const BASE_URL: &str = "http://[::1]:50052";

/// State for SSE streaming.
#[derive(Debug, Clone, Default)]
pub struct StreamingState {
    /// Response ID from the server.
    pub response_id: Option<String>,
    /// Current status.
    pub status: StreamingStatus,
    /// Accumulated output text.
    pub output: String,
    /// Error message if failed.
    pub error: Option<String>,
}

/// Status of the streaming response.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum StreamingStatus {
    #[default]
    Idle,
    Connecting,
    Streaming,
    Completed,
    Failed,
}

/// Request body for /v1/responses.
#[derive(serde::Serialize)]
struct ResponsesRequest {
    model: String,
    input: String,
    stream: bool,
}

/// Start streaming from /v1/responses endpoint.
///
/// This function:
/// 1. Makes a POST request with stream: true
/// 2. Reads the response as a ReadableStream
/// 3. Parses SSE events and calls the callback for each
/// 4. Returns when the stream is complete or fails
pub async fn stream_response<F>(
    model: &str,
    input: &str,
    mut on_event: F,
) -> Result<(), String>
where
    F: FnMut(SseEvent),
{
    let url = format!("{}/v1/responses", BASE_URL);

    // Build request body
    let body = ResponsesRequest {
        model: model.to_string(),
        input: input.to_string(),
        stream: true,
    };
    let body_json = serde_json::to_string(&body).map_err(|e| e.to_string())?;

    // Create headers
    let headers = Headers::new().map_err(|e| format!("{:?}", e))?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{:?}", e))?;

    // Create request init
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_headers(&headers);
    init.set_body(&JsValue::from_str(&body_json));
    init.set_mode(RequestMode::Cors);

    // Create request
    let request = Request::new_with_str_and_init(&url, &init)
        .map_err(|e| format!("{:?}", e))?;

    // Fetch
    let window = web_sys::window().ok_or("No window")?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{:?}", e))?
        .dyn_into()
        .map_err(|_| "Response cast failed")?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    // Get the body as ReadableStream
    let body_stream = response.body().ok_or("No response body")?;

    // Get reader
    let reader = body_stream
        .get_reader()
        .dyn_into::<web_sys::ReadableStreamDefaultReader>()
        .map_err(|_| "Failed to get reader")?;

    // Buffer for incomplete SSE events
    let mut buffer = String::new();

    // Read chunks
    loop {
        let result = JsFuture::from(reader.read())
            .await
            .map_err(|e| format!("{:?}", e))?;

        let result_obj: Object = result.dyn_into().map_err(|_| "Result not an object")?;

        // Check if done
        let done = Reflect::get(&result_obj, &JsValue::from_str("done"))
            .map_err(|_| "No done field")?
            .as_bool()
            .unwrap_or(false);

        if done {
            break;
        }

        // Get value
        let value = Reflect::get(&result_obj, &JsValue::from_str("value"))
            .map_err(|_| "No value field")?;

        if value.is_undefined() {
            continue;
        }

        // Convert Uint8Array to string
        let uint8_array: Uint8Array = value.dyn_into().map_err(|_| "Not a Uint8Array")?;
        let bytes = uint8_array.to_vec();
        let chunk = String::from_utf8_lossy(&bytes);

        // Add to buffer
        buffer.push_str(&chunk);

        // Parse complete SSE events from buffer
        while let Some(event) = extract_sse_event(&mut buffer) {
            if let Some(parsed) = event {
                on_event(parsed);
            }
        }
    }

    Ok(())
}

/// Extract one complete SSE event from the buffer.
/// Returns Some(Some(event)) if an event was found,
/// Some(None) if a blank line was found (skip),
/// None if no complete event is available yet.
fn extract_sse_event(buffer: &mut String) -> Option<Option<SseEvent>> {
    // Look for complete event (ends with \n\n)
    if let Some(end_idx) = buffer.find("\n\n") {
        let event_text = buffer[..end_idx].to_string();
        buffer.drain(..=end_idx + 1);

        // Skip comments and empty events
        if event_text.is_empty() || event_text.starts_with(':') {
            return Some(None);
        }

        // Parse event type and data
        let mut event_type = String::new();
        let mut data = String::new();

        for line in event_text.lines() {
            if let Some(value) = line.strip_prefix("event: ") {
                event_type = value.to_string();
            } else if let Some(value) = line.strip_prefix("data: ") {
                data = value.to_string();
            }
        }

        if !event_type.is_empty() && !data.is_empty() {
            return Some(SseEvent::parse(&event_type, &data));
        }

        // Skip incomplete events
        Some(None)
    } else {
        None
    }
}
