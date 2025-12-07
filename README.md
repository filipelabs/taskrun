# TaskRun

Open source control plane for orchestrating AI agents on remote workers.

## Features

- **Multi-worker orchestration**: Connect and manage multiple remote workers
- **Agent flexibility**: Run any agent logic on workers (currently supports Claude Code)
- **Model agnostic**: Use any AI model from any provider
- **Secure communication**: TLS + mTLS between control plane and workers
- **Real-time streaming**: gRPC bidirectional streaming for live output
- **CLI tool**: Manage tasks and workers from the command line
- **Observability**: Prometheus metrics, structured logging, workers UI

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
cargo run -p taskrun-control-plane
```

**Terminal 2 - Start worker:**
```bash
cargo run -p taskrun-worker
```

**Terminal 3 - Use CLI:**
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
```

### Workers UI

View connected workers in your browser:
- **HTML**: http://localhost:50052/ui/workers
- **JSON API**: http://localhost:50052/v1/workers

### Metrics

Prometheus metrics available at: http://localhost:50052/metrics

```
taskrun_workers_connected{status="idle"} 1
taskrun_workers_connected{status="busy"} 0
taskrun_tasks_total{status="pending"} 0
taskrun_tasks_total{status="running"} 1
taskrun_tasks_total{status="completed"} 5
```

## Architecture

```
┌─────────────────┐         gRPC (TLS)          ┌─────────────────┐
│    Client       │ ◄─────────────────────────►│  Control Plane  │
│   (CLI/API)     │    TaskService/Worker      │    (Server)     │
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

## Core Concepts

| Concept | Description |
|---------|-------------|
| **Task** | Logical unit of work. "Run agent X with input Y". |
| **Run** | Concrete execution of a Task on a worker. |
| **Worker** | Remote daemon that announces capabilities and executes runs. |
| **Agent** | High-level logic executed on a worker (e.g., `support_triage`). |
| **ModelBackend** | Model configuration (provider, model_name, context_window). |

## Services & Endpoints

### gRPC Services (port 50051)

| Service | Purpose |
|---------|---------|
| `TaskService` | Create, get, list, cancel tasks |
| `WorkerService` | List and query connected workers |
| `RunService` | Bidirectional streaming between workers and control plane |

### HTTP Endpoints (port 50052)

| Endpoint | Purpose |
|----------|---------|
| `GET /health` | Health check |
| `GET /metrics` | Prometheus metrics |
| `GET /v1/workers` | Workers list (JSON) |
| `GET /ui/workers` | Workers dashboard (HTML) |
| `POST /v1/enroll` | Worker certificate enrollment |

## CLI Commands

```bash
taskrun [OPTIONS] <COMMAND>

Commands:
  create-task   Create a new task
  get-task      Get task status
  list-tasks    List all tasks
  list-workers  List connected workers
  cancel-task   Cancel a task

Options:
  -a, --addr <ADDR>        Control plane address [default: https://[::1]:50051]
      --ca-cert <CA_CERT>  Path to CA certificate [default: certs/ca.crt]
```

## Project Structure

```
taskrun-v2/
  proto/                    # Protocol buffer definitions
  certs/                    # TLS certificates (generated)
  scripts/                  # Dev scripts (cert generation, demo)
  crates/
    taskrun-core/           # Domain types (Task, Run, Worker)
    taskrun-proto/          # Generated gRPC code + converters
    taskrun-control-plane/  # Control plane server
    taskrun-worker/         # Worker daemon
    taskrun-cli/            # Command line interface
    taskrun-claude-sdk/     # Claude Code SDK for agent execution
```

## Security

TaskRun uses a layered security model:

1. **TLS**: All gRPC communication is encrypted
2. **mTLS**: Workers must present valid client certificates
3. **CA-pinned**: Workers only trust the control plane's CA
4. **Short-lived certs**: Worker certificates expire in 7 days

### Worker Enrollment Flow

1. Worker generates keypair and CSR
2. Worker sends CSR + bootstrap token to `/v1/enroll`
3. Control plane validates token, signs CSR
4. Worker receives signed certificate
5. Worker connects to gRPC with mTLS

## Documentation

- [CLAUDE.md](CLAUDE.md) - Development guide for AI assistants
- [docs/project.md](docs/project.md) - Detailed design document
- [docs/security/worker-enrollment.md](docs/security/worker-enrollment.md) - Security design

## Status

Currently implemented:
- [x] Proto definitions (TaskService, WorkerService, RunService)
- [x] Control plane with in-memory storage
- [x] Worker with bidirectional streaming
- [x] Task creation, scheduling, and cancellation
- [x] Worker capability announcement
- [x] Run execution with output streaming
- [x] Backend tracking (which model was used)
- [x] CLI tool (`taskrun-cli`)
- [x] TLS/mTLS security
- [x] Prometheus metrics
- [x] Workers UI (HTML + JSON)
- [x] Structured logging with task/run correlation
- [x] Claude Code agent execution

Coming soon:
- [ ] Persistent storage (Postgres/SQLite)
- [ ] Worker certificate auto-renewal
- [ ] Multiple agent types
- [ ] Rate limiting and quotas

## License

MIT
