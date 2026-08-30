# SensoRake repository guide

Treat the Rust and TypeScript sources as the source of truth. `README.md`,
`CLAUDE.md`, `.github/copilot-instructions.md`, and
`ports-and-adapters-migration-plan.md` contain useful intent, but parts of them
lag behind the implementation.

## What the application does today

SensoRake subscribes to an MQTT topic, stores each payload in DuckDB, derives
typed temperature/humidity/pressure tables on a periodic database-worker tick,
and exposes readings, sensor mappings, health, and Prometheus metrics over
HTTP. A separate Vite/TypeScript UI calls the HTTP API; the Axum router does not
currently serve the UI assets.

Runtime flow:

```text
MQTT -> normalize timestamp + deterministic ULID -> in-memory batch
     -> DbHandle channel -> single DuckDB worker -> data_landing
     -> periodic SQL projection -> typed tables/views -> HTTP JSON
```

## Current code map

- `src/server.rs`: composition root, metrics, services, Unix signal handling.
- `src/orchestrator.rs`: starts sinks before sources; stops sources before sinks.
- `src/service.rs`: lifecycle trait and source/sink classification.
- `src/shutdown_token.rs`: shared shutdown notification.
- `src/mqtt/service.rs`: MQTT connection/event loop and the current batching
  policy (500 messages, timer, and shutdown flush).
- `src/mqtt/mqtt_buffer.rs`: payload normalization and Arrow conversion. These
  are migration candidates, not MQTT protocol concerns.
- `src/database/service.rs`: `DbHandle` plus the blocking, single-connection
  DuckDB worker. Keep DuckDB access behind this worker.
- `src/database/schema.rs`: schema, projection SQL, and query-facing views.
- `src/http/service.rs`: Axum routes. It still contains SQL and mapping
  validation that should move behind ports.
- `src/domain/`: migration-in-progress entities and ports.
- `ui/`: independent Vite/TypeScript client.

The crate currently declares the same modules from both `lib.rs` and
`main.rs`. Consequently unit tests are compiled and run once for each target.
Prefer putting implementation in the library module tree; do not add a third
copy of application logic.

## Ports-and-adapters migration

The invariant to preserve is that domain code must not import MQTT, Axum,
DuckDB, Arrow, or other adapter modules. Adapters may depend on domain types and
ports. `server.rs` owns concrete construction and dependency injection.

The migration is currently around phases 1-2: entities have moved to
`domain/entities.rs`, and output-port traits are being drafted. MQTT and HTTP
still use `DbHandle` directly; there are no repository adapters, input ports,
or domain services yet.

The revised migration plan treats these code-level mismatches as design work to
resolve during the corresponding vertical slices:

- `GET /mappings` returns the `all_sensors` projection, including `last_seen`
  and `latest_ulid`; that response is not just a list of `SensorMapping` rows.
- The three latest-reading views return measurement-specific values plus ULID,
  timestamp, battery, RSSI, and description. A generic `SensorReading` must
  represent that API deliberately rather than silently dropping fields.
- Arrow batches belong in the DuckDB adapter, while timestamp/ULID
  normalization and buffering policy belong in the application/domain layer.
- Preserve failed batches. A flush implementation must not drain and lose the
  in-memory buffer when repository storage fails.
- Existing normalization intentionally stores malformed or timestamp-less JSON
  with the current time. Decide explicitly before changing this to rejection.
- Naive timestamps are currently interpreted in the host's local timezone;
  tests should pin the intended behavior before moving this logic.
- HTTP status mapping is an adapter responsibility, but validation rules and
  typed domain errors belong inside the application/domain boundary.
- Keep module moves to `adapters/` separate from behavioral refactors to make
  review and rollback tractable.

Do not expose raw SQL, Arrow `RecordBatch`, DuckDB JSON strings, or `DbHandle`
through domain ports. Prefer domain-shaped return types and adapter-local
serialization/deserialization.

## Behavior worth preserving

- DuckDB has one owning blocking worker; async callers use `DbHandle` and
  oneshot responses.
- Startup order is database first, then MQTT/HTTP. Shutdown stops MQTT/HTTP,
  allows the final MQTT flush, then stops DuckDB.
- MQTT configuration comes from `MQTT_HOST`, `MQTT_PORT`, `MQTT_USER`,
  `MQTT_PASS`, `MQTT_TOPIC`, and `MQTT_FLUSH_INTERVAL_SECS`.
- Database configuration comes from `DUCKDB_PATH` and
  `TABLE_UPDATE_INTERVAL_SECS`.
- HTTP currently binds `0.0.0.0:3000` and provides `/temperatures`,
  `/pressures`, `/humidities`, `/mappings`, `/metrics`, and `/health`.
- Mapping deletion and restoration are idempotent soft updates returning 204.
- ULIDs are deterministic for the timestamp (millisecond precision) and raw
  payload, providing ingestion deduplication in the derived tables.

## Verification

Use focused tests while iterating, then run the relevant full gates:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
task ui:test
task test
```

DuckDB integration tests open an in-memory database; unit tests must not need a
live MQTT broker. For port migrations, add pure tests with fake repositories
and retain repository-level tests against real in-memory DuckDB.

Avoid timing-dependent sleeps where a notification, channel, or bounded
`tokio::time::timeout` can make the test deterministic.
