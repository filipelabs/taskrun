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
    taskrun-store/        # Storage traits (TaskStore, WorkerStore, RunStore)
    taskrun-store-memory/ # In-memory implementation for dev/tests
    taskrun-control-plane/# Control plane binary
    taskrun-worker/       # Worker binary
    taskrun-cli/          # Admin CLI (optional)
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

### RunService (Worker communication)

```protobuf
service RunService {
  rpc Connect(stream RunClientMessage) returns (stream RunServerMessage);
}
```

**Control plane → Worker:**
- `RunAssignment` - Assign a Run to the worker
- `CancelRun` - Cancel an in-progress Run

**Worker → Control plane:**
- `WorkerHello` + `WorkerInfo` - Announce capabilities, agents, models
- `WorkerHeartbeat` - Periodic health check
- `RunStatusUpdate` - Status changes (RUNNING, COMPLETED, FAILED)
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

# Run control plane
cargo run -p taskrun-control-plane

# Run worker
cargo run -p taskrun-worker
```

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
