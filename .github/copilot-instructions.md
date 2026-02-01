# Repository guidance for AI coding agents

This file gives concise, actionable guidance so an AI coding agent can be productive immediately in this repository.

High-level architecture
- **Purpose:** a small Rust service that listens to MQTT messages, normalizes them, persists raw/structured measurements into DuckDB (currently in-memory/file-backed), and exposes Prometheus metrics + a minimal HTTP admin UI. Ultimately intended for IoT sensor data collection and monitoring.
- **Data flow:** MQTT broker → MQTT client (`rumqttc`) → message normalization (`arrow` RecordBatches) → DuckDB (via a blocking DB worker thread) → Prometheus metrics exposed via HTTP (`axum`).
- **Configuration:** via environment variables for MQTT connection, DuckDB path, etc. Mappings between MQTT topics and DB schemas are stored in-memory and persisted to `mappings.json`. Ultimately these should be stored in DuckDB, and in the long term the user should be able to manage them configurably via the HTTP API.
- **Tech stack:**
  - Rust async runtime: `tokio`
  - MQTT client: `rumqttc`
  - HTTP server: `axum`
  - DB: `duckdb` Rust bindings
  - Metrics: `prometheus` crate
  - Message serialization: `arrow`
- **Major components:**
  - `src/server.rs` — application composition: loads mappings, registers Prometheus metrics, starts DB worker, MQTT worker, and HTTP server.
  - `src/mqtt.rs` — MQTT background worker using `rumqttc`. Reads env vars (`MQTT_HOST`, `MQTT_PORT`, `MQTT_USER`, `MQTT_PASS`, `MQTT_TOPIC`) and pushes normalized rows to the DB handle.
  - `src/mqtt_buffer.rs` — message normalization and Arrow `RecordBatch` creation. Includes unit tests that demonstrate expected message shapes.
  - `src/db.rs` — DB worker: a blocking thread owns a DuckDB `Connection` and receives `DbJob` messages via a crossbeam channel. The async `DbHandle` sends jobs and awaits oneshot responses.
  - `src/http_server.rs` — `axum` HTTP app: `PUT /mapping`, `GET /mapping`, `GET /metrics`, `GET /health`, and SPA fallback serving `ui/dist`.
  - `src/state.rs` — in-memory `Store` type (`Arc<RwLock<HashMap<...>>>`) and file-backed mapping persistence (`mappings.json`).

Key implementation patterns to follow
- Concurrency:
  - Shared in-memory state uses `type Store = Arc<RwLock<HashMap<String, Mapping>>>` (see `src/state.rs`). Prefer acquiring the lock only around immediate read/write and not across `.await` points.
  - DB access is performed by a synchronous, blocking thread (see `start_db_worker`) and addressed from async code via the `DbHandle` which sends `DbJob` objects and awaits an oneshot response. Keep that boundary intact when adding DB-related features.
- Graceful shutdown:
  - A `tokio::sync::Notify` is passed to background workers (`mqtt`, HTTP server) for coordinated shutdown (see `src/server.rs`). Use `shutdown.notified().await` to observe shutdown.
- Metrics:
  - Prometheus `Registry` is created in `server::run()` and registered into handlers as an `Extension`. Use `Registry::gather()` in `http_server::metrics_handler` to expose metrics.

Env vars and runtime notes
- Common env vars used by the service (see `src/mqtt.rs` and `src/server.rs`):
  - `MQTT_HOST`, `MQTT_PORT`, `MQTT_USER`, `MQTT_PASS`, `MQTT_TOPIC` — MQTT connection and subscription.
  - `DUCKDB_PATH` — optional path to persist DuckDB on disk; otherwise an in-memory DB is used.
- Example run (local dev):
```bash
export MQTT_TOPIC="sensors/#"
export MQTT_HOST=localhost
export MQTT_PORT=1883
# optional: export DUCKDB_PATH=./data.db
cargo run
```

Build, run, test, and UI workflows
- Rust build/run/test:
  - `cargo build` — compile
  - `cargo run` — run the server
  - `cargo test` — run Rust unit tests (see `src/mqtt_buffer.rs` and `src/db.rs` tests)
- Container: there is a `Containerfile` at the repo root. Build with your container tool of choice, for example:
  - `podman build -t rust-to-mqtt-prometheus-exporter -f Containerfile .`
- UI:
  - UI sources: `ui/src` (Vite + TypeScript). Built assets are expected under `ui/dist` and are served by the binary via `http_server::spa_handler`.
  - Dev: `cd ui && yarn && yarn dev` (Vite). Build for production: `cd ui && yarn build`.

Project-specific conventions
- Keep `main.rs` minimal — real logic lives in modules; prefer adding functionality under `src/*` and exposing a `server::run()` entry point to ease testing and hot-reload.
- Persisted mappings currently use `mappings.json` (see `src/state.rs`). When migrating to DuckDB, replace `load_mappings`/`save_mappings` with DB queries but preserve the `Store` API surface for handlers.
- DB pattern: do not attempt to use DuckDB from multiple threads concurrently — the repo centralizes DB access into a single blocking thread and communicates via channels and oneshot responses. Follow the `DbHandle` API for all DB interactions.

Files to consult for examples
- App composition and startup: `src/server.rs`
- HTTP routes and CORS behavior: `src/http_server.rs`
- MQTT connection and message loop: `src/mqtt.rs`
- Normalization + Arrow batches + tests: `src/mqtt_buffer.rs`
- DB worker + `DbHandle` examples: `src/db.rs`
- In-memory mapping store and file persistence: `src/state.rs` and `mappings.json`

If you modify or add background workers
- Ensure the worker accepts a `shutdown: Arc<Notify>` and returns a `JoinHandle` so `server::run()` can await shutdown. See `mqtt::start_mqtt_worker` signature and `http_server::start_http_server`.

When changing data persistence
- Keep the `DbHandle` contract and ensure migrations or new tables are created using `db.query("CREATE TABLE IF NOT EXISTS ...")` during startup (the MQTT worker currently does this). If you move mapping persistence to DB, update `load_mappings`/`save_mappings` accordingly and keep `Store` usage in handlers unchanged.

Testing guidance
- **Testability goal:** all modules should be unit and integration testable. When adding functionality, include tests that verify behavior at the unit level (small, fast, deterministic) and integration level (cross-module interactions, HTTP routes, DB worker behavior).
- **Unit tests:** prefer `#[cfg(test)]` modules inside the same source file for small logic (examples: `src/mqtt_buffer.rs`, `src/db.rs`). Use `#[tokio::test]` for async code. Keep business logic in small functions that return values (not directly perform IO) so they can be tested easily.
- **Integration tests:** add higher-level tests under the `tests/` directory. Examples to include:
  - HTTP handlers: construct `axum::Router` as in `src/http_server.rs` and call routes using `tower::ServiceExt::oneshot` to assert responses and status codes.
  - DB worker: reuse the existing pattern in `src/db.rs` tests — spawn a mock worker thread that receives `DbJob`s on a channel and replies, then call `DbHandle` methods to verify the async API contract.
  - Background workers: start workers with a controllable `shutdown: Arc<Notify>` and assert their behavior (e.g., final flush on shutdown). Use short timeouts in tests to avoid long waits.
- **Mocking / isolation patterns:**
  - For DB interactions, tests can provide a custom `Receiver<DbJob>` in a spawned thread (see `src/db.rs` tests) rather than opening a real DuckDB file.
  - For MQTT behavior, test normalization and batching logic independently (see `src/mqtt_buffer.rs` unit tests). Do not connect to a real broker during unit tests.
- **Async & concurrency tips:** avoid holding locks across `.await` points in production code; tests should assert no deadlocks and avoid long sleeps. Use `tokio::time::timeout` to bound async waits in tests.
- **Running tests:**
```bash
cargo test
# Run a single test with output
cargo test mqtt_buffer::tests::test_normalize_message -- --nocapture
```

What I couldn't infer automatically
- Runtime orchestration details for production (systemd, k8s manifests, secrets management) are not present. Ask the maintainer for preferred deployment patterns and required secrets handling.

If anything here is unclear or you want a different level of detail (examples, expanded env var matrix, or suggested tests), tell me which sections to expand.
