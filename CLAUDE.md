# CLAUDE.md - TaskRun v2 Development Guide

This file provides context for AI assistants working on the TaskRun codebase.

## Project Overview

**TaskRun** is an open source control plane for orchestrating multiple AI agents running on remote workers. It enables a central server to manage:

- Multiple **remote workers**
- Multiple **agents** (different logic/flows)
- Multiple **models** (any provider, any format)

Communication between control plane and workers uses **gRPC bidirectional streaming** over HTTP/2, with strong security (mTLS, pinned CA, code signing).

> **Note**: This is TaskRun v2. Version 1 was internal-only and never released publicly.

## Core Concepts

| Concept | Description |
|---------|-------------|
| **Task** | Logical unit of work in the control plane. Represents user intent: "run agent X with input Y". |
| **Run** | Concrete execution of a Task on a specific worker. Multiple Runs can exist per Task (retry, fan-out). |
| **Worker** | Remote daemon that maintains persistent connection, announces capabilities, receives Runs, streams output. |
| **Agent** | High-level logic (flow/orchestration) executed on a worker. Control plane doesn't know implementation details. |
| **ModelBackend** | Abstraction for "model behind an agent". Fields: provider, model_name, context_window, modalities, etc. |

### Control Plane vs Data Plane

- **Control Plane**: Where Tasks live, routing decisions are made, global view of workers and Runs.
- **Data Plane**: Where execution happens (workers and their models).

## Architecture

```
┌─────────────────┐         gRPC/HTTP          ┌─────────────────┐
│    Client       │ ◄─────────────────────────►│  Control Plane  │
│   (CLI/API)     │        TaskService         │    (Server)     │
└─────────────────┘                            └────────┬────────┘
                                                        │
                                          gRPC Streaming│Bidi
                                              (mTLS)    │
                                                        ▼
                                               ┌────────────────┐
                                               │    Workers     │
                                               │  (Agent Runs)  │
                                               └────────────────┘
```

## Crate Layout

```
taskrun-v2/
  Cargo.toml              # workspace
  proto/                  # .proto files (source of truth)
  crates/
    taskrun-core/         # Domain types (Task, Run, Worker, ModelBackend)
    taskrun-proto/        # Generated gRPC code (tonic/prost) + converters
    taskrun-server/       # Control plane server (TUI or --headless daemon)
    taskrun-worker/       # Worker binary (TUI or --headless daemon)
    taskrun-tui-components/           # Shared TUI components (widgets, theme, utils)
    taskrun-cli/          # Admin CLI
    taskrun-claude-sdk/   # Claude Code SDK for agent execution
```

## gRPC Services

### TaskService (Client-facing API)

```protobuf
service TaskService {
  rpc CreateTask(CreateTaskRequest) returns (Task);
  rpc GetTask(GetTaskRequest) returns (Task);
  rpc ListTasks(ListTasksRequest) returns (ListTasksResponse);
  rpc CancelTask(CancelTaskRequest) returns (Task);
}
```

### WorkerService (Query workers)

```protobuf
service WorkerService {
  rpc ListWorkers(ListWorkersRequest) returns (ListWorkersResponse);
  rpc GetWorker(GetWorkerRequest) returns (Worker);
}
```

### RunService (Worker ↔ Control Plane streaming)

```protobuf
service RunService {
  rpc StreamConnect(stream RunClientMessage) returns (stream RunServerMessage);
}
```

**Control plane → Worker:**
- `RunAssignment` - Assign a Run to the worker
- `CancelRun` - Cancel an in-progress Run

**Worker → Control plane:**
- `WorkerHello` + `WorkerInfo` - Announce capabilities, agents, models
- `WorkerHeartbeat` - Periodic health check
- `RunStatusUpdate` - Status changes (RUNNING, COMPLETED, FAILED) + `backend_used`
- `RunOutputChunk` - Streaming tokens/content

## Status Enums

### TaskStatus
`PENDING` → `RUNNING` → `COMPLETED` | `FAILED` | `CANCELLED`

### RunStatus
`PENDING` → `ASSIGNED` → `RUNNING` → `COMPLETED` | `FAILED` | `CANCELLED`

## Security Requirements

These are **non-negotiable** for the control plane ↔ worker channel:

1. **TLS with pinned CA**: Workers trust only the control plane's CA, not system CAs
2. **mTLS**: Mutual authentication - workers present client certificates
3. **Short-lived certificates**: Worker certs expire quickly (24h-7d), auto-renewal
4. **Enrollment flow**: Bootstrap token + CSR → signed worker certificate
5. **Code signing**: If control plane distributes code/flows, they must be signed
6. **Replay protection**: Unique `run_id`, timestamps, nonce validation

## Project Management

### GitHub Project & Issues

All work is tracked in the GitHub Project:
- **Project Board**: https://github.com/orgs/filipelabs/projects/1
- **Issues**: https://github.com/filipelabs/taskrun/issues

### Workflow Rules

1. **Every feature or fix MUST be linked to a GitHub Issue**
   - Do not start work without an associated issue
   - If an issue doesn't exist, create one first

2. **Branch naming convention**
   ```
   <type>/<issue-number>-<short-description>
   ```
   Examples:
   - `feat/5-run-assignment-fake`
   - `fix/12-worker-heartbeat-timeout`
   - `docs/25-readme-update`

3. **Commit messages MUST reference the issue**
   ```
   <type>: <description>

   Refs #<issue-number>
   ```
   Or to auto-close:
   ```
   <type>: <description>

   Closes #<issue-number>
   ```

4. **Pull Requests**
   - Title: `[#<issue>] <description>`
   - Body must link to the issue with `Closes #<issue>` or `Refs #<issue>`
   - PRs without linked issues will not be merged

### Issue Labels

| Label | Description |
|-------|-------------|
| `proto` | Protocol buffer definitions |
| `control-plane` | Control plane server |
| `worker` | Worker daemon |
| `scheduler` | Task scheduling logic |
| `security` | mTLS, certificates, auth |
| `obs` | Observability (metrics, logs) |
| `docs` | Documentation |
| `done` | Migrated from Linear as completed |

### Before Starting Work

1. Check the project board for prioritized issues
2. Assign yourself to the issue
3. **Move the issue to "In Progress" on the project board** (required)
4. Create a branch following the naming convention
5. When done, close the issue (or create a PR linking the issue)

### When Closing Issues

- Always close issues with a comment explaining what was done
- If the issue was already implemented, explain where/when
- Reference the commit SHA if applicable

## Development Guidelines

### Proto-First Development

1. Define messages and services in `.proto` files first
2. Generate Rust code with `tonic-build`
3. Implement converters between proto types and domain types

### Domain Isolation

`taskrun-core` must have **zero dependencies** on:
- Network/gRPC
- Database
- Runtime specifics

It contains only pure domain types and logic.

### Storage Abstraction

All persistence goes through traits:

```rust
#[async_trait]
pub trait TaskStore: Send + Sync {
    async fn create(&self, task: Task) -> Result<Task>;
    async fn get(&self, id: &TaskId) -> Result<Option<Task>>;
    async fn update_status(&self, id: &TaskId, status: TaskStatus) -> Result<()>;
    async fn list(&self) -> Result<Vec<Task>>;
}
```

In-memory implementations first, then Postgres/SQLite later.

### Terminology Consistency

Always use the official terms:
- `Task` (not "job", "request", "work item")
- `Run` (not "execution", "instance")
- `Worker` (not "node", "executor", "runner")
- `Agent` (not "bot", "assistant", "flow")
- `ModelBackend` (not "provider", "llm", "model" alone)

When introducing new concepts, integrate them into existing vocabulary rather than inventing new terms.

### TUI Development (taskrun-tui-components)

The `taskrun-tui-components` crate provides shared TUI components used by both `taskrun-server` and `taskrun-tui`.

**Architecture:**
```
taskrun-tui-components/
  src/
    lib.rs          # Public exports
    theme.rs        # Colors, styles (Theme struct)
    utils.rs        # Text wrapping, formatting helpers
    widgets/        # Reusable ratatui widgets
      mod.rs
      chat.rs       # ChatWidget - conversation display
      events.rs     # EventsWidget - execution events
```

**Rules for taskrun-tui-components:**

1. **No domain dependencies**: `taskrun-tui-components` must NOT depend on `taskrun-core`, `taskrun-proto`, or any other TaskRun crate. It only depends on `ratatui`, `crossterm`, `chrono`, and `unicode-width`.

2. **Data-agnostic widgets**: Widgets receive data through their own simple structs (e.g., `ChatMessage`, `EventInfo`), not domain types. Consumers convert domain types to widget types.

3. **Builder pattern**: Widgets use builder pattern for configuration:
   ```rust
   ChatWidget::new(&messages)
       .streaming(&current_output)
       .scroll(offset)
       .focused(true)
       .render(frame, area);
   ```

4. **Theme consistency**: All widgets accept a `Theme` for styling. Use `Theme::default()` for standard TaskRun colors.

5. **When to add to taskrun-tui-components**:
   - Widget is used by 2+ TUI applications
   - Widget is generic (not tied to specific domain logic)
   - Widget handles common patterns (chat, logs, events, tables)

6. **When NOT to add to taskrun-tui-components**:
   - Application-specific layouts or views
   - Widgets that need domain types directly
   - One-off UI components

**Using widgets in applications:**

```rust
// In taskrun-server or taskrun-tui
use taskrun_ui::{ChatWidget, ChatMessage, ChatRole, Theme};

// Convert domain types to widget types
let messages: Vec<ChatMessage> = domain_messages
    .iter()
    .map(|m| ChatMessage {
        role: match m.role {
            taskrun_core::ChatRole::User => ChatRole::User,
            taskrun_core::ChatRole::Assistant => ChatRole::Assistant,
            taskrun_core::ChatRole::System => ChatRole::System,
        },
        content: m.content.clone(),
        timestamp: m.timestamp,
    })
    .collect();

// Render widget
ChatWidget::new(&messages)
    .focused(state.chat_focused)
    .render(frame, area);
```

## Common Flows

### Worker Registration

1. Worker starts → connects via mTLS to `RunService.Connect`
2. Sends `WorkerHello` with `WorkerInfo` (agents, model backends, labels)
3. Control plane registers/updates worker in `WorkerStore`
4. Worker sends periodic `WorkerHeartbeat`

### Task Execution

1. Client calls `TaskService.CreateTask` with agent_name, input_json, labels
2. Control plane creates Task (status: `PENDING`)
3. Scheduler finds compatible worker (has requested agent)
4. Creates Run, sends `RunAssignment` to worker
5. Worker executes agent, streams `RunOutputChunk`
6. Worker sends final `RunStatusUpdate` (COMPLETED/FAILED)
7. Control plane updates Task status

### Task Cancellation

1. Client calls `TaskService.CancelTask`
2. Control plane marks Task as CANCELLED
3. Sends `CancelRun` to worker(s) with active Runs
4. Workers stop execution, send `RunStatusUpdate(CANCELLED)`

## Build Commands

```bash
# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Generate proto code (handled by build.rs in taskrun-proto)
cargo build -p taskrun-proto

# Run server (TUI)
cargo run -p taskrun-server

# Run server (headless daemon)
cargo run -p taskrun-server -- --headless

# Run worker
cargo run -p taskrun-worker
```

## Testing End-to-End Execution

### Prerequisites

1. **Claude Code CLI** must be installed and authenticated
2. **Certificates** must be generated (see `certs/` directory)
3. **grpcurl** for making gRPC calls (optional, for manual testing)

### Quick Start (Three Terminals)

**Terminal 1 - Server:**
```bash
cargo run -p taskrun-server
```

**Terminal 2 - Worker:**
```bash
RUST_LOG=info cargo run -p taskrun-worker
```

**Terminal 3 - Create Task:**
```bash
# Using grpcurl (requires mTLS certs)
grpcurl -cacert certs/ca.crt -cert certs/worker.crt -key certs/worker.key \
  -import-path proto -proto taskrun/v1/task_service.proto \
  -d '{"agent_name":"support_triage","input_json":"{\"ticket_id\":\"TEST-123\",\"subject\":\"Cannot login\"}"}' \
  '[::1]:50051' taskrun.v1.TaskService/CreateTask

# Check task status
grpcurl -cacert certs/ca.crt -cert certs/worker.crt -key certs/worker.key \
  -import-path proto -proto taskrun/v1/task_service.proto \
  -d '{"task_id":"<TASK_ID_FROM_ABOVE>"}' \
  '[::1]:50051' taskrun.v1.TaskService/GetTask
```

### Expected Logs (Success)

```
# Control Plane
INFO Creating task task_id=<uuid> agent=support_triage
INFO Assigning task to worker task_id=<uuid> run_id=<uuid> worker_id=<uuid>
INFO Run status update status=Running
INFO Output chunk received content_len=252
INFO Run status update status=Completed
INFO Task completed

# Worker
INFO Received run assignment run_id=<uuid> agent=support_triage
INFO Starting real execution via Claude Code
INFO Claude process spawned successfully
INFO Received message from Claude message_num=1 bytes=3766
INFO StreamingHandler received message message_type="System"
INFO Captured session ID session_id=<uuid>
INFO StreamingHandler received message message_type="Assistant"
INFO Streaming assistant text chunk text_len=252
INFO StreamingHandler received message message_type="Result"
INFO Execution result received is_error=Some(false) duration_ms=Some(4549)
INFO Claude process exited exit_code=0 success=true
INFO Real execution completed successfully
```

### Available Agents

| Agent | Description | Input JSON |
|-------|-------------|------------|
| `support_triage` | Classifies support tickets | `{"ticket_id": "...", "subject": "...", "body": "..."}` |

### Troubleshooting

| Issue | Solution |
|-------|----------|
| `CertificateRequired` error | Use mTLS certs with grpcurl: `-cacert`, `-cert`, `-key` flags |
| Worker not connecting | Ensure control plane is running first |
| Claude not found | Verify `claude` CLI is in PATH: `which claude` |
| Unknown message types | Check Claude Code version, may need update |
| Task stays PENDING | No worker available with requested agent |

## Code Style

- Use `rustfmt` defaults
- Prefer explicit error types over `anyhow` in library crates
- Use `thiserror` for error definitions
- Async runtime: `tokio`
- gRPC: `tonic` + `prost`
- Logging: `tracing`

## File Naming

- Proto files: `snake_case.proto`
- Rust modules: `snake_case.rs`
- IDs: Always wrap in newtype (`TaskId(String)`, `RunId(String)`, `WorkerId(String)`)
