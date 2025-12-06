# TaskRun

Open source control plane for orchestrating AI agents on remote workers.

## Features

- **Multi-worker orchestration**: Connect and manage multiple remote workers
- **Agent flexibility**: Run any agent logic on workers
- **Model agnostic**: Use any AI model from any provider
- **Secure communication**: mTLS between control plane and workers
- **Real-time streaming**: gRPC bidirectional streaming for live output

## Quick Start

```bash
# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace
```

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

## Documentation

- [CLAUDE.md](CLAUDE.md) - Development guide
- [docs/project.md](docs/project.md) - Detailed design document

## License

MIT
