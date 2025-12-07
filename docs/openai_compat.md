# TaskRun – OpenAI-Compatible HTTP Layer

## Overview

TaskRun exposes an HTTP API compatible with the OpenAI Responses API, allowing clients to use familiar OpenAI SDKs while internally orchestrating agents on remote workers.

**Key principle:** Outside it looks like "just another OpenAI-style API", inside TaskRun orchestrates agents, workers, and Claude Code.

## Architecture

```
Client (OpenAI SDK / curl)
        │
        ▼
POST /v1/responses
        │
        ▼
┌───────────────────────────────────────────┐
│  Control Plane HTTP (axum)                │
│                                           │
│  responses_openai.rs:                     │
│    1. Parse OpenAI-style request          │
│    2. Resolve model → agent_name          │
│    3. Create Task                         │
│    4. Schedule Run on available Worker    │
│    5. Wait for completion (poll)          │
│    6. Build OpenAI-style response         │
└───────────────────────────────────────────┘
        │
        ▼ gRPC bidirectional stream (mTLS)
        │
┌───────────────────────────────────────────┐
│  Worker                                   │
│    - Receives RunAssignment               │
│    - Executes agent via Claude Code       │
│    - Streams output chunks back           │
│    - Reports completion/failure           │
└───────────────────────────────────────────┘
```

## Endpoint

### `POST /v1/responses`

Creates a response by executing an agent on a worker.

#### Request

```json
{
  "model": "general",
  "input": "What is 2 + 2?",
  "instructions": "You are a helpful assistant.",
  "stream": false,
  "max_output_tokens": 512,
  "temperature": 0.2,
  "metadata": {
    "tenant": "acme",
    "request_id": "abc-123"
  }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `model` | string | Yes | Agent identifier. Can be `"general"` or `"taskrun:general"` |
| `input` | string or array | Yes | The prompt/task. String for simple input, array for structured messages |
| `instructions` | string | No | System instructions for the agent |
| `stream` | boolean | No | Enable SSE streaming (not yet implemented) |
| `max_output_tokens` | integer | No | Maximum tokens in response |
| `temperature` | float | No | Sampling temperature |
| `metadata` | object | No | Arbitrary key-value pairs, stored as task labels |

#### Response (Non-Streaming)

```json
{
  "id": "resp_d50ec052-2dd2-4395-ac5e-e0c69dc127c6",
  "object": "response",
  "created_at": 1741476542,
  "status": "completed",
  "model": "general",
  "output": [
    {
      "type": "message",
      "id": "msg_83ca52da-c884-41a7-aef8-b4fd1e6bafc0",
      "role": "assistant",
      "status": "completed",
      "content": [
        {
          "type": "output_text",
          "content_type": "text/plain",
          "text": "4"
        }
      ]
    }
  ],
  "usage": {
    "input_tokens": 12,
    "output_tokens": 3,
    "total_tokens": 15
  },
  "metadata": {
    "task_id": "83ca52da-c884-41a7-aef8-b4fd1e6bafc0",
    "run_id": "d50ec052-2dd2-4395-ac5e-e0c69dc127c6",
    "worker_id": "5beb7de9-98bc-4ee4-beee-cefdcc651fcd"
  }
}
```

| Field | Description |
|-------|-------------|
| `id` | Response ID, format: `resp_<run_id>` |
| `object` | Always `"response"` |
| `created_at` | Unix timestamp |
| `status` | `"in_progress"`, `"completed"`, `"failed"`, `"cancelled"` |
| `model` | Echoed from request |
| `output` | Array of output items (messages) |
| `usage` | Token usage (when available) |
| `error` | Error details (when `status` is `"failed"`) |
| `metadata` | Internal IDs for debugging |

#### Content Types

Output content blocks include `content_type` for future multimodal support:

| Type | content_type | Description |
|------|--------------|-------------|
| `output_text` | `text/plain` | Plain text output |
| `output_json` | `application/json` | Structured JSON (future) |
| `output_audio` | `audio/ogg` | Audio output (future) |

#### Response (Streaming)

When `stream: true`, the response is Server-Sent Events (SSE):

```
event: response.created
data: {"id":"resp_abc123","object":"response","model":"general","created_at":1741476542}

event: response.output_text.delta
data: {"response_id":"resp_abc123","output_index":0,"delta":{"content_type":"text/plain","text":"The answer is "}}

event: response.output_text.delta
data: {"response_id":"resp_abc123","output_index":0,"delta":{"content_type":"text/plain","text":"4"}}

event: response.completed
data: {"id":"resp_abc123","status":"completed","output":[],"usage":null}
```

**Events:**
| Event | Description |
|-------|-------------|
| `response.created` | Emitted immediately when the response starts |
| `response.output_text.delta` | Emitted for each output chunk from the agent |
| `response.completed` | Emitted when execution completes successfully |
| `response.failed` | Emitted when execution fails or is cancelled |

**Notes:**
- The stream terminates after `response.completed` or `response.failed`
- Output is streamed via delta events; the final `output` array in `response.completed` is empty
- Keep-alive comments are sent periodically to maintain the connection

## Error Responses

Errors follow OpenAI format:

```json
{
  "error": {
    "message": "Streaming is not yet implemented",
    "type": "invalid_request_error",
    "code": "streaming_not_supported"
  }
}
```

| HTTP Status | Type | When |
|-------------|------|------|
| 400 | `invalid_request_error` | Invalid request, unknown model |
| 401 | `authentication_error` | Invalid/missing API key (planned) |
| 429 | `rate_limit_error` | Rate limit exceeded (planned) |
| 500 | `internal_error` | Server error |
| 504 | `timeout_error` | Task execution timeout |

## Model → Agent Mapping

The `model` field maps to TaskRun agents:

| Model | Agent | Description |
|-------|-------|-------------|
| `general` | `general` | General-purpose Claude Code agent |
| `taskrun:general` | `general` | Same, with explicit prefix |
| `support_triage` | `support_triage` | Support ticket classification |

The `taskrun:` prefix is optional and stripped during resolution.

## Internal Flow

1. **Request Parsing**: Extract `model`, `input`, and optional parameters
2. **Agent Resolution**: Map `model` to `agent_name`
3. **Input Building**: Construct `input_json` with task and metadata
4. **Task Creation**: Create Task with agent_name and input_json
5. **Scheduling**: Assign Task to available Worker via Scheduler
6. **Execution**: Worker executes via Claude Code, streams output
7. **Completion**: Control plane collects output, builds response
8. **Response**: Return OpenAI-formatted JSON

## Configuration

No additional configuration required. The endpoint uses existing:
- Worker pool (connected via gRPC)
- Agent definitions (from workers)
- Task/Run storage (in-memory)

## Usage Examples

### curl (non-streaming)

```bash
curl -X POST http://[::1]:50052/v1/responses \
  -H "Content-Type: application/json" \
  -d '{"model":"general","input":"What is 2+2?"}'
```

### curl (streaming)

```bash
curl -N -X POST http://[::1]:50052/v1/responses \
  -H "Content-Type: application/json" \
  -d '{"model":"general","input":"What is 2+2?","stream":true}'
```

The `-N` flag disables buffering for real-time SSE output.

### Python (OpenAI SDK style)

```python
import requests

response = requests.post(
    "http://localhost:50052/v1/responses",
    json={
        "model": "general",
        "input": "Explain quantum computing briefly."
    }
)
print(response.json()["output"][0]["content"][0]["text"])
```

## Limitations (Current)

- **No authentication**: Bearer token auth not yet implemented
- **No rate limiting**: No request throttling
- **No token counting**: `usage` field not populated
- **5-minute timeout**: Fixed execution timeout for non-streaming requests

## Future Enhancements

1. **Bearer Token Authentication**: `Authorization: Bearer <api_key>`
2. **Token Usage Tracking**: Populate `usage` field
3. **Rate Limiting**: Per-tenant request limits
4. **Structured Input**: Full message array support
