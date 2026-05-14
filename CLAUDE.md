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
# Run all tests (Rust + UI)
cargo test
task test

# Run Rust tests only
cargo test

# Run a single test with output
cargo test <test_name> -- --nocapture

# Run UI tests
cd ui && yarn test
task ui:test

# Run UI tests in watch mode
cd ui && yarn test --watch
task ui:test:watch

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
yarn test         # Run Vitest unit tests
yarn test --watch # Run tests in watch mode
yarn test:ui      # Run tests with interactive UI dashboard
```

### Task Commands (Taskfile.dev)
```bash
task ui:build       # Build the UI (yarn build)
task ui:dev         # Start UI dev server (yarn dev)
task ui:test        # Run UI tests (yarn test)
task ui:test:watch  # Run UI tests in watch mode
task ui:test:ui     # Run tests with interactive UI
task build          # Build Rust + UI together
task test           # Run all tests (Rust + UI)
task dev            # Start UI dev server
```

## Architecture Overview

**Purpose:** A Rust service that subscribes to MQTT topics, normalizes sensor messages, persists them to DuckDB, and exposes Prometheus metrics via HTTP with a minimal web UI for managing sensor mappings.

**Data Flow:** MQTT → normalize to Arrow RecordBatches → DuckDB worker → Prometheus metrics + HTTP API + Web UI

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

## HTTP API

### Sensor Mappings Endpoints

The `/mappings` endpoints allow creating, reading, and managing sensor-to-description mappings. Mappings can be soft-deleted and restored.

**GET `/mappings`**
- Returns all sensors with their mapping information
- Response: JSON array of sensor objects with optional `mapping_id`, `description`, `validity_start`, and `deleted` flag
- Used by the UI to display all sensors grouped by state (mapped, unmapped, deleted)

**POST `/mappings`**
- Creates a new sensor mapping
- Request body: `{"model": "string", "id": "string", "validity_start": "ISO8601-UTC", "description": "string"}`
- Response (201 Created): `{"mapping_id": number, "model": "string", "id": "string", "validity_start": "ISO8601", "description": "string"}`
- All fields are required; `validity_start` must be a valid ISO 8601 timestamp in UTC format
- Example: `POST /mappings` with body `{"model":"TempSensor","id":"001","validity_start":"2025-02-14T10:00:00Z","description":"Living Room"}`

**DELETE `/mappings/{id}`**
- Soft-deletes a mapping (marks with `deleted=true` flag)
- Path parameter: `id` is the `mapping_id` from POST response
- Response (204 No Content)
- Soft-deletes preserve the record for auditing and restoration

**POST `/mappings/{id}/restore`**
- Restores a soft-deleted mapping (clears the `deleted` flag)
- Path parameter: `id` is the `mapping_id` to restore
- Response (204 No Content)

### Other Endpoints

**GET `/temperatures`, `/pressures`, `/humidities`**
- Returns latest readings by type
- Response: JSON array of sensor readings

**GET `/metrics`**
- Prometheus metrics in text format
- Used by Prometheus scraper

**GET `/health`**
- Health check endpoint
- Response (200 OK): empty body

## UI Architecture

The UI is a Vanilla TypeScript + DOM application (no framework) using Vite for development and building, and Vitest for testing.

### UI File Structure

| File | Purpose |
|------|---------|
| `ui/src/types.ts` | Type definitions for sensors, mappings, and UI state enums |
| `ui/src/api.ts` | `ApiClient` class abstracting HTTP calls to backend REST API |
| `ui/src/ui.ts` | UI component classes: `SensorListUI` (displays sensors) and `CreateMappingFormUI` (form for new mappings) |
| `ui/src/main.ts` | Entry point: initializes UI, wires up event handlers, loads sensors on startup |
| `ui/src/style.css` | CSS with design tokens (colors from logo), responsive design |
| `ui/index.html` | HTML structure with header logo, form container, and sensor list container |

### UI Component Architecture

**SensorListUI** (`ui/src/ui.ts`)
- Renders sensors grouped by state: "Active Mappings" (mapped), "Unmapped Sensors", "Deleted Mappings"
- Unmapped sensor names are clickable (underlined, teal text) and trigger pre-filling the form
- Mapped sensors have delete buttons; deleted sensors have restore buttons
- Displays sensor metadata (validity_start, description)

**CreateMappingFormUI** (`ui/src/ui.ts`)
- Form with required fields: Sensor Model, Sensor ID, Valid From (datetime-local), Description
- `selectSensor(model, id)` method pre-fills model and ID fields and focuses description (called when clicking unmapped sensor)
- `getFormData()` validates all fields are filled and converts local datetime to ISO 8601 UTC
- `resetForm()` clears form and resets datetime to now

**ApiClient** (`ui/src/api.ts`)
- Async methods: `getSensors()`, `createMapping()`, `deleteMapping()`, `restoreMapping()`
- All errors are thrown as `Error` with descriptive messages
- Base URL configurable via environment or defaults to `http://localhost:3000`

### UI Data Flow

1. **Load**: `main.ts` calls `loadSensors()` on startup via `apiClient.getSensors()`
2. **Render**: `SensorListUI.render()` groups sensors by `getSensorState()` and renders sections
3. **User creates mapping**: Form submission → `handleCreateMapping()` → `apiClient.createMapping()` → reload sensors
4. **User clicks unmapped sensor**: `createMappingUI.selectSensor()` → pre-fill form with model/id → focus description
5. **User deletes/restores**: Confirmation → `apiClient.deleteMapping()`/`restoreMapping()` → reload sensors

### Testing

- UI has 25+ unit tests covering:
  - Sensor state detection (`getSensorState()`)
  - Form validation and data extraction
  - Component rendering (sections, buttons with correct data attributes)
  - Unmapped sensor clickability
  - Form pre-filling via `selectSensor()`

Run with `yarn test` or `task ui:test`

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

**Query with parameters:**
```rust
let json = db_handle.query_with_params(
    "SELECT * FROM mappings WHERE id = ?".to_string(),
    vec!["001".to_string()]
).await?;
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

### REST API Patterns: Soft Deletes
- Soft-deleted records remain in the database with a `deleted = true` flag
- GET /mappings returns both active and deleted records (UI filters and displays separately)
- DELETE marks as deleted (204 No Content); POST /restore unmarks the flag (204 No Content)
- This enables auditing and recovery of accidentally deleted mappings

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

**Web UI Implementation:**
- Created vanilla TypeScript + Vite frontend with Vitest unit tests
- `SensorListUI` and `CreateMappingFormUI` components for managing mappings
- Clicking unmapped sensor names pre-fills form for quick mapping creation
- All 25+ UI tests passing; build outputs optimized assets to `ui/dist`

**REST API for Mappings:**
- Implemented `/mappings` endpoints: GET (list all), POST (create), DELETE (soft-delete), and POST /restore (undelete)
- `SensorMapping` struct with required `validity_start: DateTime<Utc>` field
- Soft-delete pattern: DELETE marks with `deleted=true`, allows restoration via restore endpoint
- All timestamps enforce RFC 3339 UTC format (enforced at type level via `DateTime<Utc>`)

**Database Query Serialization:**
- Queries in the database service are serialized to JSON (not raw Arrow RecordBatches)
- Uses `arrow-json` crate's `ArrayWriter` in the DB worker thread to convert `RecordBatch` → JSON
- JSON format is `[{"col1": val1, "col2": val2}, ...]` for easy consumption by JavaScript frontend
- This keeps formatting logic centralized in the DB layer; HTTP handlers just pass through the JSON string

## Important Files

- `Cargo.toml` — Dependencies and build profiles; note `arrow-json = "56.2.0"` for query serialization
- `Containerfile` — Docker/Podman build definition
- `pre-commit` — Shell script for fmt and clippy checks (run before committing)
- `Taskfile.yml` — Task automation for common workflows (build, test, dev)
- `ui/` — Vite + TypeScript frontend with unit tests; built assets served from `ui/dist`
- `ui/vitest.config.ts` — Vitest configuration with jsdom environment for DOM testing

## Environment-Specific Notes

- **Local development:** Use `cargo run` with environment variables for MQTT broker; use `task dev` or `yarn dev` to start UI dev server
- **Container builds:** Use `podman build -f Containerfile .` or equivalent
- **Pre-commit hook:** Enable with `git config core.hooksPath .` and `chmod +x pre-commit` (or `.git/hooks/pre-commit`)
- **UI dependencies:** Added `jsdom` to `devDependencies` for Vitest DOM environment
