# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Essential Commands

### Building and Running
```bash
# Build the Rust binary
cargo build

# Run the service locally (defaults to listening on http://127.0.0.1:3000)
cargo run

# With environment variables (example)
MQTT_HOST=localhost MQTT_PORT=1883 MQTT_TOPIC="sensors/#" cargo run
```

### Testing and Linting
```bash
# Run all tests
cargo test

# Run a single test with output
cargo test <test_name> -- --nocapture

# Format check (runs in pre-commit)
cargo fmt --all -- --check

# Lint check (runs in pre-commit)
cargo clippy --all-targets --all-features -- -D warnings

# Run pre-commit checks manually
./pre-commit
```

### UI Development
```bash
cd ui
yarn              # Install dependencies
yarn dev          # Start Vite dev server (http://localhost:5173)
yarn build        # Build for production (outputs to ui/dist)
```

## Architecture Overview

**Purpose:** A Rust service that subscribes to MQTT topics, normalizes sensor messages, persists them to DuckDB, and exposes Prometheus metrics via HTTP with a minimal web UI.

**Data Flow:** MQTT → normalize to Arrow RecordBatches → DuckDB worker → Prometheus metrics + HTTP API

### Core Design Patterns

**Service Orchestration Pattern** (`src/orchestrator.rs`, `src/service.rs`):
- All background workers (MQTT, HTTP, Database) implement the `Service` trait with `ServiceType` (Source or Sink)
- The `Orchestrator` manages startup and graceful shutdown
- Sources (MQTT, HTTP) start after Sinks (Database) are ready
- Reverse order on shutdown: Sources stop first, then Sinks

**Graceful Shutdown** (`src/shutdown_token.rs`):
- `ShutdownToken` is a cloneable async signal passed to all services
- Services call `token.wait().await` to receive shutdown notifications
- The orchestrator triggers `token.trigger()` which notifies all waiters

**Database Worker Pattern** (`src/database/`):
- DuckDB runs in a single blocking thread to ensure thread safety (DuckDB is not multi-threaded safe)
- Async code communicates via channels: sends `DbJob` messages and awaits oneshot responses
- `DbHandle` is the async-friendly API; never call DuckDB directly from async code

**Configuration:**
- MQTT: `MQTT_HOST`, `MQTT_PORT`, `MQTT_USER`, `MQTT_PASS`, `MQTT_TOPIC`
- Database: `DUCKDB_PATH` (optional; defaults to in-memory)
- Logging: `RUST_LOG` (e.g., `RUST_LOG=info cargo run`)

### Module Overview

| Module | Purpose |
|--------|---------|
| `src/main.rs` | Entry point; delegates to `server::run()` |
| `src/server.rs` | Application composition: initializes services, registry, and orchestrator |
| `src/orchestrator.rs` | Manages service startup/shutdown lifecycle |
| `src/service.rs` | Service trait definition (Source/Sink types) |
| `src/mqtt/` | MQTT client worker (`MqttService`) |
| `src/database/` | DuckDB worker thread and handle (`DbService`) |
| `src/http/` | HTTP server routes and handlers (`HttpService`) |
| `src/state.rs` | Placeholder for future mapping storage (currently unused) |
| `src/shutdown_token.rs` | Graceful shutdown signaling mechanism |

## Key Implementation Patterns

### Adding a New Service
1. Implement the `Service` trait in a new module
2. Define `svc()` to return `ServiceType::Source` or `ServiceType::Sink`
3. Implement `start()` to spawn a background task and return `JoinHandle`
4. Use `ShutdownToken` to listen for shutdown: `token.wait().await`
5. Register the service in `server.rs` before calling `orchestrator.start_all()`

### Database Access
Always use the `DbHandle` API from async code:

**Query data (returns JSON string):**
```rust
let json = db_handle.query("SELECT * FROM temperatures").await?;
// json is a String containing JSON array: [{"col1": val1, ...}, ...]
```

**Insert batches (accepts Arrow RecordBatch):**
```rust
let batch = create_record_batch()?;
db_handle.insert_batch(batch, "data_landing").await?;
```

**Implementation details:**
- Database queries use DuckDB's `query_arrow()` to get results as Arrow `RecordBatch`
- `arrow-json` crate converts `RecordBatch` to JSON array format in the DB worker
- Queries return JSON strings (not Arrow objects) to keep formatting logic centralized in the DB layer
- Never call DuckDB connection methods directly from async code—the worker thread enforces this boundary
- The whitelist in `handle_db_job()` prevents SQL injection for inserts (only allows `data_landing` table)

### HTTP Handlers
- Handlers receive `Extension<Arc<Registry>>` (Prometheus) and `Extension<DbHandle>` (database)
- Keep handler logic minimal; delegate business logic to service functions
- Use `Extension` for all shared state
- Routes are defined in `build_router()` which takes `DbHandle` and `Registry` as parameters
- Use `tower::util::ServiceExt::oneshot()` in tests to call routes (see `src/http/service.rs` test patterns)

### Testing
- **Unit tests:** Place inline with `#[cfg(test)]` modules in the same file (see `src/orchestrator.rs`, `src/database/service.rs`)
- **Integration tests:** Add to `tests/` directory
- **Pattern:** Use `#[tokio::test]` for async tests, avoid real MQTT/DuckDB connections in unit tests
- **Shutdown testing:** Use `tokio::time::timeout` to bound async waits and prevent test hangs

**Database handler testing pattern:**
When testing code that uses `DbHandle`, create a mock worker in a spawned thread:
```rust
let (tx, rx) = unbounded::<DbJob>();
let handle = DbHandle::new(tx);  // DbHandle::new is pub(crate), accessible within tests

// Spawn a mock or real DB worker thread
std::thread::spawn(move || {
    while let Ok(job) = rx.recv() {
        // Handle DbCommand::Query or DbCommand::InsertBatch
        // Send response via job.response
    }
});

// Now use handle.query() or handle.insert_batch() in your test
```
See `src/database/service.rs` test module for complete examples (`test_insert_batch_roundtrip`, `test_query_returns_json`).

## Recent Changes & Current Status

**Database Query Serialization:**
- Queries in the database service are serialized to JSON (not raw Arrow RecordBatches)
- Uses `arrow-json` crate's `ArrayWriter` in the DB worker thread to convert `RecordBatch` → JSON
- JSON format is `[{"col1": val1, "col2": val2}, ...]` for easy consumption by JavaScript frontend
- This keeps formatting logic centralized in the DB layer; HTTP handlers just pass through the JSON string

**HTTP Service Refactoring (in progress):**
- The `temperatures_handler` is currently commented out; uncomment and implement using `db_handle.query()` when ready
- Test scaffold pattern in `src/http/service.rs` shows how to create `build_test_app()` with a mock `DbHandle`

## Important Files

- `Cargo.toml` — Dependencies and build profiles; note `arrow-json = "56.2.0"` for query serialization
- `Containerfile` — Docker/Podman build definition
- `pre-commit` — Shell script for fmt and clippy checks (run before committing)
- `ui/` — Vite + TypeScript frontend; built assets served from `ui/dist`

## Environment-Specific Notes

- **Local development:** Use `cargo run` with environment variables for MQTT broker
- **Container builds:** Use `podman build -f Containerfile .` or equivalent
- **Pre-commit hook:** Enable with `git config core.hooksPath .` and `chmod +x pre-commit` (or `.git/hooks/pre-commit`)
