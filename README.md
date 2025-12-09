# TaskRun

Open source control plane for orchestrating AI agents on remote workers.

## Features

- **Multi-worker orchestration**: Connect and manage multiple remote workers
- **Agent flexibility**: Run any agent logic on workers (currently supports Claude Code)
- **Model agnostic**: Use any AI model from any provider
- **OpenAI-compatible API**: Drop-in replacement with `/v1/responses` endpoint
- **Real-time streaming**: SSE and gRPC bidirectional streaming for live output
- **Secure communication**: TLS + mTLS between control plane and workers
- **MCP integration**: Model Context Protocol server for AI assistant tool use
- **CLI, TUI & DevTools**: Command-line interface, terminal dashboard, and Tauri desktop app
- **Observability**: Prometheus metrics, structured logging, workers dashboard

## Quick Start

### Prerequisites

- Rust 1.75+
- [Claude Code CLI](https://claude.ai/claude-code) (for agent execution)

### Build

```bash
cargo build --workspace
```

### Generate TLS Certificates

```bash
./scripts/gen-dev-certs.sh
```

This creates:
- `certs/ca.crt` / `certs/ca.key` - Certificate Authority
- `certs/server.crt` / `certs/server.key` - Control plane TLS
- `certs/worker.crt` / `certs/worker.key` - Worker mTLS client cert

### Run

**Terminal 1 - Start control plane:**
```bash
RUST_LOG=info cargo run -p taskrun-control-plane
```

**Terminal 2 - Start worker:**
```bash
RUST_LOG=info cargo run -p taskrun-worker
```

**Terminal 3 - Test with HTTP API:**
```bash
# Simple request
curl -X POST http://[::1]:50052/v1/responses \
  -H "Content-Type: application/json" \
  -d '{"model": "general", "input": "What is 2+2?"}'

# With streaming
curl -X POST http://[::1]:50052/v1/responses \
  -H "Content-Type: application/json" \
  -d '{"model": "general", "input": "Write a hello world in Python", "stream": true}'
```

## HTTP API

### POST /v1/responses (OpenAI-compatible)

Create a task and wait for completion. Compatible with OpenAI's Responses API format.

**Request:**
```json
{
  "model": "general",
  "input": "Explain quantum computing",
  "instructions": "Be concise",
  "stream": false,
  "max_output_tokens": 4096,
  "temperature": 0.7,
  "metadata": {"user_id": "123"}
}
```

**Response:**
```json
{
  "id": "resp_01abc...",
  "object": "response",
  "created_at": 1701234567,
  "status": "completed",
  "output": [
    {
      "type": "message",
      "role": "assistant",
      "content": [{"type": "output_text", "text": "..."}]
    }
  ],
  "model": "claude-sonnet-4-20250514",
  "usage": {"input_tokens": 50, "output_tokens": 200}
}
```

**Streaming (SSE):**
When `stream: true`, returns Server-Sent Events:
```
data: {"type":"response.output_text.delta","delta":"Hello"}
data: {"type":"response.output_text.delta","delta":" world"}
data: {"type":"response.completed","response":{...}}
```

**Error Responses:**
```json
{
  "error": {
    "message": "Missing required field: model",
    "type": "invalid_request_error",
    "code": "missing_field",
    "param": "model"
  }
}
```

| Status | Code | Description |
|--------|------|-------------|
| 400 | `invalid_json` | Malformed JSON body |
| 400 | `missing_field` | Required field not provided |
| 400 | `invalid_field` | Field value out of range |
| 400 | `model_not_found` | No worker supports the model/agent |
| 503 | `no_workers_available` | Workers offline or at capacity |
| 504 | `task_timeout` | Execution exceeded deadline |

### Other HTTP Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check (returns `{"status": "ok"}`) |
| `/metrics` | GET | Prometheus metrics |
| `/v1/workers` | GET | Workers list (JSON) |
| `/ui/workers` | GET | Workers dashboard (HTML) |
| `/v1/enroll` | POST | Worker certificate enrollment |
| `/v1/tasks/:id/events` | GET | Run events for a task (JSON) |
| `/v1/tasks/:id/output` | GET | Task output stream (SSE) |
| `/mcp` | POST | MCP server (Streamable HTTP transport) |

## TUI (Terminal User Interface)

Interactive terminal interfaces for monitoring and operating TaskRun.

### Server TUI

Run the control plane server with an integrated dashboard:

```bash
cargo run -p taskrun-server
```

Features:
- Workers view - connected workers and their status
- Tasks view - task list with status and details
- Logs view - real-time server logs
- Run detail view - chat interface for interacting with tasks

### Worker TUI

Run a worker with interactive terminal UI:

```bash
cargo run -p taskrun-worker
```

Features:
- Setup screen for agent and model selection
- Real-time connection status and run monitoring
- Chat interface for runs
- Live log streaming
- Auto-reconnection with exponential backoff

```bash
# With custom options
cargo run -p taskrun-worker -- \
  --agent general \
  --model claude-sonnet-4-5 \
  --endpoint https://[::1]:50051

# Headless mode (daemon, no TUI)
cargo run -p taskrun-worker -- --headless
```

## CLI

```bash
# List connected workers
cargo run -p taskrun-cli -- list-workers

# Create a task
cargo run -p taskrun-cli -- create-task \
  --agent support_triage \
  --input '{"subject": "Cannot login", "body": "I forgot my password"}'

# Get task status
cargo run -p taskrun-cli -- get-task <task-id>

# List all tasks
cargo run -p taskrun-cli -- list-tasks

# Cancel a task
cargo run -p taskrun-cli -- cancel-task <task-id>
```

## MCP Server

TaskRun exposes an MCP (Model Context Protocol) server that allows AI assistants like Claude to interact with the control plane.

**Available Tools:**

| Tool | Description |
|------|-------------|
| `list_workers` | List connected workers and their capabilities |
| `start_new_task` | Create and start a new task on an available worker |
| `get_task` | Get task details including status, output, and chat history |
| `continue_task` | Continue an existing task with a follow-up message |

**Claude Code Configuration:**

Add to your MCP settings:
```json
{
  "mcpServers": {
    "taskrun": {
      "type": "streamable-http",
      "url": "http://[::1]:50052/mcp"
    }
  }
}
```

The MCP server supports session continuation, allowing multi-turn conversations with tasks.

## DevTools

Desktop application for monitoring and testing TaskRun.

```bash
cd devtools
cargo tauri dev
```

Features:
- Worker status monitoring
- Task creation and tracking
- Real-time output streaming
- Playground for testing prompts

## Architecture

```
┌─────────────────┐         gRPC (TLS)          ┌─────────────────┐
│    Client       │ ◄─────────────────────────►│  Control Plane  │
│  (HTTP/CLI)     │    TaskService/Worker      │    (Server)     │
└─────────────────┘                            └────────┬────────┘
                                                        │
                                          gRPC Streaming│Bidi
                                              (mTLS)    │
                                                        ▼
                                               ┌────────────────┐
                                               │    Workers     │
                                               │  (Claude Code) │
                                               └────────────────┘
```

**Ports:**
- `50051` - gRPC server (TLS/mTLS for workers and CLI)
- `50052` - HTTP server (API, UI, metrics)

## Core Concepts

| Concept | Description |
|---------|-------------|
| **Task** | Logical unit of work. "Run agent X with input Y". |
| **Run** | Concrete execution of a Task on a worker. Tasks can have multiple runs (retry, fan-out). |
| **Worker** | Remote daemon that announces capabilities and executes runs. |
| **Agent** | High-level logic executed on a worker (e.g., `support_triage`, `general`). |
| **ModelBackend** | Model configuration (provider, model_name, context_window). |
| **RunEvent** | Execution stage tracking (session init, tool use, output). |

### Status Flow

**Task:** `PENDING` → `RUNNING` → `COMPLETED` | `FAILED` | `CANCELLED`

**Run:** `PENDING` → `ASSIGNED` → `RUNNING` → `COMPLETED` | `FAILED` | `CANCELLED`

## gRPC Services

| Service | Methods | Description |
|---------|---------|-------------|
| `TaskService` | CreateTask, GetTask, ListTasks, CancelTask | Task management |
| `WorkerService` | ListWorkers, GetWorker | Worker queries |
| `RunService` | StreamConnect (bidirectional) | Worker ↔ Control plane streaming |

### Worker Protocol

**Worker → Control Plane:**
- `WorkerHello` - Announces capabilities (agents, backends)
- `WorkerHeartbeat` - Periodic health check (15s interval)
- `RunStatusUpdate` - Status changes + `backend_used`
- `RunOutputChunk` - Streaming output with sequence numbers
- `RunEvent` - Execution stage events

**Control Plane → Worker:**
- `RunAssignment` - Task assignment with input and deadline
- `CancelRun` - Cancel a specific run

## Metrics

Prometheus format at `http://[::1]:50052/metrics`:

```
# Workers by status
taskrun_workers_connected{status="idle"} 1
taskrun_workers_connected{status="busy"} 0
taskrun_workers_connected{status="draining"} 0
taskrun_workers_connected{status="error"} 0

# Tasks by status
taskrun_tasks_total{status="pending"} 0
taskrun_tasks_total{status="running"} 1
taskrun_tasks_total{status="completed"} 5
taskrun_tasks_total{status="failed"} 0
taskrun_tasks_total{status="cancelled"} 0
```

## Project Structure

```
taskrun/
├── proto/                      # Protocol buffer definitions
│   └── taskrun/v1/
│       ├── common.proto        # Shared types (Status, ModelBackend, AgentSpec)
│       ├── task_service.proto  # TaskService RPC
│       ├── worker_service.proto # WorkerService RPC
│       └── run_service.proto   # RunService bidirectional streaming
├── certs/                      # TLS certificates (generated)
├── scripts/                    # Dev scripts (cert generation)
├── devtools/                   # Tauri + Leptos desktop app
└── crates/
    ├── taskrun-core/           # Domain types (Task, Run, Worker)
    ├── taskrun-proto/          # Generated gRPC code + converters
    ├── taskrun-control-plane/  # Control plane library (state, services)
    ├── taskrun-server/         # Server TUI binary
    ├── taskrun-worker/         # Worker binary (headless or --tui)
    ├── taskrun-ui/             # Shared TUI components
    ├── taskrun-cli/            # Command line interface
    └── taskrun-claude-sdk/     # Claude Code SDK for agent execution
```

## Security

TaskRun uses a layered security model:

1. **TLS**: All gRPC communication is encrypted
2. **mTLS**: Workers must present valid client certificates
3. **CA-pinned**: Workers only trust the control plane's CA
4. **Short-lived certs**: Worker certificates expire in 7 days

### Worker Enrollment Flow

```
1. Worker generates keypair and CSR
2. Worker sends CSR + bootstrap token to POST /v1/enroll
3. Control plane validates token, signs CSR with CA
4. Worker receives signed certificate (7-day validity)
5. Worker connects to gRPC with mTLS
```

Generate additional worker certificates:
```bash
./scripts/gen-worker-cert.sh worker2
```

## Configuration

### Control Plane

| Setting | Default | Description |
|---------|---------|-------------|
| `bind_addr` | `[::1]:50051` | gRPC server address |
| `http_bind_addr` | `[::1]:50052` | HTTP server address |
| `heartbeat_timeout_secs` | `45` | Worker timeout before removal |
| `worker_cert_validity_days` | `7` | Enrolled cert validity |

### Worker

| Setting | Default | Description |
|---------|---------|-------------|
| `control_plane_addr` | `https://[::1]:50051` | Control plane URL |
| `heartbeat_interval_secs` | `15` | Heartbeat frequency |
| `reconnect_delay_secs` | `5` | Reconnect backoff |
| `max_concurrent_runs` | `10` | Parallel execution limit |
| `claude_path` | `claude` | Claude CLI binary |

### Environment Variables

```bash
RUST_LOG=info          # Logging level (trace, debug, info, warn, error)
```

## Available Agents

| Agent | Description | Input Format |
|-------|-------------|--------------|
| `general` | General-purpose Claude Code agent | Any text prompt |
| `support_triage` | Classify support tickets | `{"subject": "...", "body": "..."}` |

## Documentation

- [CLAUDE.md](CLAUDE.md) - Development guide for AI assistants
- [docs/project.md](docs/project.md) - Detailed design document
- [docs/security/worker-enrollment.md](docs/security/worker-enrollment.md) - Security design

## Status

**Implemented:**
- [x] Proto definitions (TaskService, WorkerService, RunService)
- [x] Control plane with in-memory storage
- [x] Worker with bidirectional streaming
- [x] Task creation, scheduling, and cancellation
- [x] Run execution with output streaming
- [x] Backend tracking (which model was used)
- [x] TLS/mTLS security with CA enrollment
- [x] OpenAI-compatible `/v1/responses` endpoint
- [x] SSE streaming for real-time output
- [x] Error handling with proper HTTP status codes
- [x] CLI tool (`taskrun-cli`)
- [x] TUI dashboard (control plane monitoring + interactive worker)
- [x] DevTools desktop app (Tauri + Leptos)
- [x] Prometheus metrics
- [x] Workers UI dashboard
- [x] Run events tracking
- [x] Claude Code SDK integration
- [x] MCP server for AI assistant integration

**Roadmap:**
- [ ] Persistent storage (Postgres/SQLite)
- [ ] Worker certificate auto-renewal
- [ ] Bearer token authentication for HTTP API
- [ ] Token usage tracking
- [ ] Rate limiting and quotas
- [ ] Structured input arrays (multi-turn messages)

## License

MIT
