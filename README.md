# TaskRun

Open source control plane for orchestrating AI agents on remote workers.

## Features

- **Multi-worker orchestration**: Connect and manage multiple remote workers
- **Agent flexibility**: Run any agent logic on workers
- **Model agnostic**: Use any AI model from any provider
- **Secure communication**: mTLS between control plane and workers (planned)
- **Real-time streaming**: gRPC bidirectional streaming for live output

## Quick Start

### Prerequisites

- Rust 1.75+
- [grpcurl](https://github.com/fullstorydev/grpcurl) (for testing)

### Build

```bash
cargo build --workspace
```

### Run Demo

**Terminal 1 - Start control plane:**
```bash
cargo run -p taskrun-control-plane
```

**Terminal 2 - Start worker:**
```bash
cargo run -p taskrun-worker
```

**Terminal 3 - Create a task:**
```bash
# List connected workers
grpcurl -plaintext -import-path proto -proto taskrun/v1/worker_service.proto \
  '[::1]:50051' taskrun.v1.WorkerService/ListWorkers

# Create a task
grpcurl -plaintext -import-path proto -proto taskrun/v1/task_service.proto \
  -d '{"agent_name": "support_triage", "input_json": "{\"query\": \"help\"}"}' \
  '[::1]:50051' taskrun.v1.TaskService/CreateTask

# Get task status (replace <task-id>)
grpcurl -plaintext -import-path proto -proto taskrun/v1/task_service.proto \
  -d '{"id": "<task-id>"}' \
  '[::1]:50051' taskrun.v1.TaskService/GetTask
```

## Architecture

```
┌─────────────────┐         gRPC/HTTP          ┌─────────────────┐
│    Client       │ ◄─────────────────────────►│  Control Plane  │
│   (CLI/API)     │    TaskService/Worker      │    (Server)     │
└─────────────────┘                            └────────┬────────┘
                                                        │
                                          gRPC Streaming│Bidi
                                             RunService │
                                                        ▼
                                               ┌────────────────┐
                                               │    Workers     │
                                               │  (Agent Runs)  │
                                               └────────────────┘
```

## Core Concepts

| Concept | Description |
|---------|-------------|
| **Task** | Logical unit of work. "Run agent X with input Y". |
| **Run** | Concrete execution of a Task on a worker. |
| **Worker** | Remote daemon that announces capabilities and executes runs. |
| **Agent** | High-level logic executed on a worker (e.g., `support_triage`). |
| **ModelBackend** | Model configuration (provider, model_name, context_window). |

## gRPC Services

| Service | Purpose |
|---------|---------|
| `TaskService` | Create, get, list, cancel tasks |
| `WorkerService` | List and query connected workers |
| `RunService` | Bidirectional streaming between workers and control plane |

## Project Structure

```
taskrun-v2/
  proto/                    # Protocol buffer definitions
  crates/
    taskrun-core/           # Domain types (Task, Run, Worker)
    taskrun-proto/          # Generated gRPC code + converters
    taskrun-control-plane/  # Control plane server
    taskrun-worker/         # Worker daemon
```

## Documentation

- [CLAUDE.md](CLAUDE.md) - Development guide for AI assistants
- [docs/project.md](docs/project.md) - Detailed design document

## Status

Currently implemented:
- [x] Proto definitions (TaskService, WorkerService, RunService)
- [x] Control plane with in-memory storage
- [x] Worker with bidirectional streaming
- [x] Task creation and scheduling
- [x] Worker capability announcement
- [x] Run execution with output streaming
- [x] Backend tracking (which model was used)

Coming soon:
- [ ] CLI tool (`taskrun ctl`)
- [ ] TLS/mTLS security
- [ ] Persistent storage
- [ ] Metrics and observability

## License

MIT
