# Ports and Adapters Migration Plan

## Purpose

This document guides an incremental refactoring of SensoRake. The application
already has a working MQTT-to-DuckDB-to-HTTP path; the goal is to make its
behavior clearer and easier to test without replacing that working path in one
large rewrite.

The architecture is a means, not the product. Introduce a port when it protects
a meaningful use case or enables a valuable test. Do not add a trait merely to
mirror every existing function.

The Rust and TypeScript sources remain the source of truth. Each migration
slice must preserve observable behavior unless the slice explicitly introduces
and tests a deliberate behavior change.

## Product Scope

SensoRake:

1. receives readings from inexpensive IoT sensors, currently through MQTT;
2. preserves every received message as raw history;
3. normalizes supported environmental measurements;
4. lets a user join changing physical-device identities into stable logical
   sensor histories;
5. exposes current logical readings for Prometheus scraping; and
6. provides HTTP/UI administration for device discovery and assignments.

Historical storage may later be useful for analytics or AI/ML, but AI/ML
workflows are not part of this project.

## Agreed Domain Model

The central problem is continuity of measurement series despite unstable
physical identities. A cheap sensor can report a new identifier after a reset
or battery change. SensoRake must regard the new identifier as a newly observed
physical device and let the user manually connect its measurement series to an
existing logical sensor.

```text
received payload
      |
      +--> immutable RawMessage
      |
      +--> PhysicalDevice + Observations
                              |
                    time-bounded SeriesBindings
                              |
                              v
                    LogicalSensor "Livingroom"
                       |             |
                  temperature    humidity
```

### RawMessage

The immutable input evidence. Every payload is stored even when it is
malformed, unsupported, duplicated, or belongs to an unassigned device.

It should contain at least:

- an ingestion identifier;
- the original payload;
- `received_at`;
- the adapter/source context needed for diagnostics, such as MQTT topic; and
- an optional parsed `observed_at` when one can be obtained.

`received_at` is controlled by SensoRake and drives discovery/staleness.
`observed_at` comes from the reading and drives measurement history. Their
fallback and validation rules must be explicit.

### PhysicalDevice

A system record for one observed transmitter identity. For the current input
format its natural identity is based on reported fields such as model, ID, and
channel. Channel must participate when present. Source namespace may need to
participate later if multiple receivers can produce colliding identities.

An ID change creates a new `PhysicalDevice`; the system does not infer that it
is the same hardware. Continuity is established manually through bindings.

Device diagnostics belong here:

- current/most recently reported battery state;
- current/most recently reported RSSI;
- `first_seen` and `last_seen`; and
- the measurement kinds this device has produced.

Battery and RSSI are persisted as physical-device diagnostics. They do not
form the logical sensor's environmental measurement continuity.

### MeasurementKind and Observation

`MeasurementKind` initially covers supported environmental quantities such as
temperature, humidity, and pressure. It must have a canonical unit per kind at
the domain boundary.

An `Observation` belongs to a `PhysicalDevice` and one `MeasurementKind`. It
contains the normalized value and observation time. One payload can produce
several observations.

### LogicalSensor

A stable user concept such as "Livingroom". One logical sensor can contain
multiple measurement series and those series may come from different physical
devices. Use a stable internal ID distinct from the user-facing display name so
renaming does not rewrite history.

If a Prometheus label is intended to remain stable across display-name changes,
introduce an immutable user-readable key in addition to the internal ID and
display name. This can be deferred until the Prometheus contract is designed,
but it must not be accidentally equated with mutable free text.

### SeriesBinding

A time-bounded association from one physical device's measurement kind to the
same measurement kind on a logical sensor:

```text
(physical_device_id, measurement_kind)
    -> (logical_sensor_id, measurement_kind)
    during [valid_from, valid_until)
```

Binding individual series rather than whole devices supports cases such as:

- device A supplies Livingroom temperature and humidity;
- device B later replaces only Livingroom temperature; and
- device A remains the authoritative Livingroom humidity source.

### Domain Invariants

1. Raw messages are never discarded because parsing or mapping fails.
2. Bindings are explicitly time-bounded; `valid_until = None` means open-ended.
3. At most one physical series is authoritative for a logical sensor and
   measurement kind at any instant.
4. A physical series should not feed multiple logical sensors over the same
   interval.
5. Creating a conflicting binding never silently shortens or replaces an
   existing binding.
6. Resolving a conflict is an explicit user action: close the old interval,
   choose another logical sensor, leave the new series unassigned, or change
   the requested interval.
7. Mapping changes do not mutate raw messages or physical observations. They
   change how observations are interpreted as logical history.
8. Automatic identity-change detection is out of scope. The system may later
   suggest a relationship, but must not create one without confirmation.

## Discovery and Administration

The administration UI should make physical-device lifecycle visible without
pretending to know hardware identity:

- **new**: recently first seen and not fully assigned;
- **active**: seen within a configured interval;
- **stale**: not seen for a displayed duration;
- **partially assigned**: some environmental series are bound and others are
  not; and
- **replaced**: relevant bindings have ended.

These are mostly query/read-model states derived from `first_seen`, `last_seen`,
reported measurement kinds, and bindings. Avoid persisting redundant status
flags unless a status has independent domain meaning.

When a user binds a new temperature series to an interval already occupied by
an old one, return a typed conflict containing enough information for the UI to
offer explicit choices. Do not automatically disconnect the old series.

## Prometheus and History Semantics

DuckDB and Prometheus serve different needs:

- DuckDB holds raw messages, normalized physical observations, devices,
  logical sensors, bindings, and historical interpretation.
- Prometheus exposes the latest authoritative logical values. It is not the
  primary historical store for this application.
- Unassigned observations remain in DuckDB but are not exported as ordinary
  logical sensor metrics. Device discovery/diagnostic metrics may be designed
  separately with careful label-cardinality limits.
- Battery and RSSI may be shown in the administration API/UI as physical-device
  diagnostics; they are not values of `Livingroom.temperature`, for example.

The Prometheus metric names, labels, staleness behavior, and handling of
temporarily missing series require a small explicit contract before exporter
implementation.

## Architectural Boundaries

```text
DRIVING ADAPTERS
  MQTT consumer       HTTP administration       Prometheus HTTP scrape
         \                    |                         /
          \                   |                        /
           v                  v                       v
APPLICATION INPUT PORTS / USE CASES
  ingest payloads   discover devices   manage logical sensors/bindings
                              export latest logical readings
                              |
                              v
DOMAIN
  identities, observations, logical sensors, bindings, interval rules
                              |
                              v
APPLICATION OUTPUT PORTS
  ingestion persistence   sensor catalog/binding persistence   read models
                              |
                              v
DRIVEN ADAPTERS
  DuckDB repositories through the single-owner DbHandle worker
```

Dependency rules:

- Domain code imports no MQTT, Axum, Prometheus, DuckDB, Arrow, SQL, JSON
  serialization, environment configuration, or service-lifecycle modules.
- Application services depend on domain types and output-port traits.
- Driving adapters translate external requests into input-port calls and map
  typed application errors into protocol responses.
- Driven adapters implement output ports and own SQL, Arrow conversion, and
  database serialization.
- `server.rs` is the composition root and may know every concrete type.
- `Service`, `Orchestrator`, and `ShutdownToken` are runtime infrastructure,
  not domain concepts.

Input and output ports should be shaped around use cases, not database tables.
Keep them narrow, but do not split them so aggressively that every method gets
its own trait.

## Current State

The current repository already has several foundations worth preserving:

- `DbHandle` sends jobs to one blocking worker that owns the DuckDB connection;
- `Service` and `Orchestrator` order startup and shutdown;
- MQTT performs threshold, timer, and shutdown flushes;
- all payloads land in `data_landing`;
- DuckDB SQL derives typed measurement tables and latest-value views;
- HTTP supports latest-reading and mapping operations; and
- a Vite/TypeScript UI consumes the mapping API.

The ports-and-adapters work is only partially started. Some entities have moved
to `src/domain/entities.rs`, and draft output traits exist, but HTTP and MQTT
still depend directly on `DbHandle`. Treat those draft types as disposable
design notes rather than contracts that must be preserved.

Known correctness and design gaps to address during migration:

- mapping creation sends four parameters through a database method that only
  supports up to three;
- existing HTTP integration tests do not exercise the router-to-database path;
- `GET /mappings` is really a combined discovered-device/mapping projection;
- the current mapping identity ignores channel;
- current mapping views use description as part of temporal identity;
- delete and restore suppress database errors;
- timestamp parsing uses the host local timezone for naive timestamps;
- malformed or timestamp-less payloads silently receive the current time;
- Arrow and SQL representation details leak into MQTT and HTTP;
- flush logic is repeated and must retain messages when persistence fails; and
- `main.rs` redeclares the library modules, compiling unit tests twice.

## Migration Strategy

Migrate in vertical, reviewable slices. Do not build every final directory and
trait before connecting one complete use case. Each phase below must leave the
application runnable.

### Phase 0: Establish Characterization Tests - *DONE*

Before moving boundaries, capture the behavior that is meant to survive.

- [x] Add real Axum router tests that use a controlled `DbHandle` worker or an
   in-memory DuckDB-backed application.
- [x] Add a test proving mapping creation reaches DuckDB and returns its generated
   ID; fix the four-parameter limitation as part of that test.
- [x] Characterize MQTT normalization for numeric epoch timestamps, naive
   timestamps, malformed JSON, missing timestamps, and deterministic IDs.
- [x] Test threshold, timer, shutdown, and failed-storage flush behavior around a
   small extracted batching helper if necessary.
- [x] Stop declaring the application module tree from both `main.rs` and `lib.rs`;
   let the binary call the library composition root.

Gate: `cargo fmt`, strict Clippy, and all Rust tests pass. Tests document any
temporarily preserved behavior that is scheduled to change later.

### Phase 1: Introduce the Domain Vocabulary - *DONE*

Replace the current transport/database-shaped draft entities with the agreed
domain concepts, initially without rewiring runtime behavior:

- [x] stable ID newtypes;
- [x] `PhysicalDeviceIdentity` and `PhysicalDevice`;
- [x] `MeasurementKind`;
- [x] `Observation`;
- [x] `LogicalSensor`;
- [x] `SeriesBinding` with half-open intervals; and
- [x] typed validation/conflict errors.

Keep HTTP request/response DTOs in the HTTP adapter. Keep DuckDB row structures
in the DuckDB adapter. Derive Serde on domain types only when serialization is
actually part of the domain contract; otherwise translate explicitly.

Add pure tests for interval overlap, open-ended intervals, identity equality
including optional channel, and binding conflicts.

Gate: no runtime behavior change; pure domain tests pass without Tokio or
DuckDB.

### Phase 2: Migrate Device Discovery and Existing Mapping Behavior

This is the first complete vertical slice.

#### Phase 2.1: Name the compatibility slice

The current `/mappings` API mixes two concepts:

- discovered physical devices from `data_landing` / `all_sensors`; and
- user-assigned descriptions from the old `mappings` table.

In Phase 2, keep this behavior working but name it honestly. Treat it as a
temporary discovered-device assignment view, not as the final `SeriesBinding`
model.

- [x] Define a typed read model for one discovered physical device row returned
  by today's `all_sensors` projection. Include the existing response fields:
  `mapping_id`, `model`, `id`, `last_seen`, `latest_ulid`, `description`,
  `validity_start`, and `deleted`.
- [x] Decide whether this compatibility read model lives in the application
  layer or in a temporary compatibility module. Do not put SQL-shaped DTOs in
  `domain`.
- [x] Record in code comments that this read model wraps legacy mapping
  behavior and should be replaced by physical-device/logical-sensor/binding
  read models in Phase 3.

#### Phase 2.2: Define application input use cases

Start with use cases that preserve existing HTTP behavior. Avoid adding use
cases for the final binding model yet.

- [x] Define an input-port/application service method to list discovered
  devices with their current legacy assignment state.
- [x] Define an input-port/application service method to assign a discovered
  physical identity to a logical display name using the current legacy
  `mappings` table.
- [x] Define input-port/application service methods to soft-delete and restore
  one legacy assignment, preserving the current idempotent `204` behavior.
- [x] Confirm that a separate operation for listing logical sensor
  names/descriptions is not needed by the current UI; defer it until required.
- [x] Keep validation rules in the application layer: non-empty model,
  reported ID, and display name/description; duplicate assignment conflicts;
  and typed not-found/conflict/validation errors where behavior is no longer
  intentionally idempotent.

#### Phase 2.3: Define only the output operations those use cases need

The output port should describe what the application needs, not how DuckDB
executes it.

- [x] Add a narrow output trait for the compatibility assignment slice, for
  example operations shaped like:
  - list discovered devices with assignment state;
  - create a legacy assignment;
  - soft-delete a legacy assignment by ID;
  - restore a legacy assignment by ID; and
  - list known logical display names/descriptions, if needed.
- [x] Confirm that a separate operation for listing logical sensor
  names/descriptions is not needed by the current UI; defer it until required.
- [x] Return typed structs from the output port. Do not return DuckDB JSON
  strings, raw SQL rows, or `serde_json::Value`.
- [x] Keep `DbHandle`, SQL strings, DuckDB parameter binding, and JSON
  deserialization out of the application/domain boundary.
- [x] Write application-service tests against a fake output port before
  changing the HTTP handlers.

#### Phase 2.4: Implement the DuckDB adapter

Keep DuckDB behind its existing single-owner worker. This phase adapts
`DbHandle`; it does not replace it.

- [ ] Implement the output trait with a DuckDB-backed adapter that uses
  `DbHandle`.
- [ ] Move the existing `/mappings` SQL from `src/http/service.rs` into this
  adapter.
- [ ] Parse `DbHandle` JSON responses into typed adapter/application structs
  inside the adapter.
- [ ] Preserve current SQL behavior unless a deliberate behavior change is
  listed and tested.
- [ ] Add repository/adapter tests using real in-memory DuckDB.

#### Phase 2.5: Move HTTP handlers onto input ports

HTTP should become a driving adapter: translate requests into input-port calls,
then translate typed results/errors into HTTP responses.

- [ ] Change router state from concrete `DbHandle` to the application input
  port needed by the mapping/discovery handlers.
- [ ] Keep HTTP request and response DTOs in `src/http`.
- [ ] Translate HTTP payloads into application commands.
- [ ] Translate typed application errors into HTTP status codes:
  validation errors to `400`, duplicate assignment conflicts to `409`,
  unexpected infrastructure errors to `500`, and idempotent delete/restore to
  `204` as today.
- [ ] Update existing router tests to use fake input ports.
- [ ] Retain at least one router-to-in-memory-DuckDB integration test for the
  full compatibility slice.

#### Phase 2.6: Update the UI only after the backend seam is clear

- [ ] Keep the current UI behavior working against the compatibility API while
  the backend seam is introduced.
- [ ] If UI wording changes during this phase, prefer vocabulary such as
  discovered device, assignment, and logical display name over final
  `SeriesBinding` language.
- [ ] Defer per-measurement-kind binding UI until Phase 3.

During this phase, the old `mappings` table may be wrapped temporarily. Do not
force it to masquerade as the final series-binding model. Record any temporary
compatibility translation and remove it in Phase 3.

Gate: handler tests use fake input ports; repository tests use real in-memory
DuckDB; at least one end-to-end router-to-DuckDB test covers the slice.

### Phase 3: Add Time-Bounded Series Bindings

Introduce the actual continuity model.

- [ ] Add logical-sensor and series-binding schema migrations.
- [ ] Store one binding per physical device, measurement kind, logical sensor, and half-open validity interval.
- [ ] Enforce overlap invariants in the application service and, where practical, protect them transactionally in persistence.
- [ ] Add use cases to create a binding, close it at a selected time, list binding history, and inspect conflicts.
- [ ] Make conflict responses structured enough for the UI to present choices.
- [ ] Migrate existing mapping records deliberately; because old records do not distinguish measurement kinds, expansion into per-kind bindings may require inspecting observed capabilities or user confirmation.

Soft deletion should not substitute for temporal history. Correcting erroneous
administrative records may still require an audit/cancellation mechanism, but
normal replacement means ending an interval.

Gate: tests cover non-overlapping succession, conflicts, partial device
assignment, temperature replacement while humidity remains, and explicit
closure of an open binding.

### Phase 4: Extract Loss-Safe Ingestion

Separate MQTT transport from ingestion policy and DuckDB representation.

1. MQTT receives bytes, topic, and receipt time and calls an ingestion input
   port.
2. An application ingestion service preserves the raw message and attempts
   normalization into device observations and diagnostics.
3. Normalization is pure and has an explicit result that can represent both a
   preserved raw message and parse issues.
4. Batching policy has an injected threshold and one `flush()` path used by
   threshold, timer, and shutdown triggers.
5. A failed flush retains or restores the exact batch for retry.
6. Arrow `RecordBatch` construction moves into the DuckDB ingestion adapter.
7. Prefer one output operation capable of atomically storing the raw record and
   associated normalized results when normalization succeeds.

The ingestion service must not know MQTT. MQTT reconnection, subscription, and
QoS remain adapter concerns. Timer scheduling and Prometheus transport counters
also remain outside the domain.

Gate: fake-persistence tests cover successful flush, failed flush without data
loss, retry, malformed payload preservation, multiple observations per payload,
and device diagnostic updates. MQTT unit tests need no broker.

### Phase 5: Build Logical History Read Models

Replace description-based SQL views with queries that resolve observations
through time-bounded bindings.

1. Query logical history by joining observation time into `[valid_from,
   valid_until)`.
2. Query latest authoritative values per logical sensor and measurement kind.
3. Keep physical-device observations queryable independently for discovery and
   troubleshooting.
4. Expose typed application results to HTTP; serialize only in the adapter.
5. Decide pagination/range limits before exposing unbounded history endpoints.

Gate: repository tests prove history continuity across a reported-ID change,
device replacement for only one measurement kind, and no duplication at an
interval boundary.

### Phase 6: Expose Sensor Values for Prometheus

Design the exporter contract before implementing it.

1. Choose metric names and canonical units.
2. Choose stable, bounded labels based on logical sensor identity.
3. Define when a reading becomes stale and whether it disappears or is
   accompanied by a freshness metric.
4. Query latest logical readings through an application input port.
5. Keep exporter encoding and Prometheus registry details in the HTTP/metrics
   adapter.
6. Keep MQTT/ingestion health metrics separate from logical sensor readings.

Gate: text-format tests verify names, labels, units, values, and staleness;
unassigned physical devices cannot create unbounded logical-series labels.

### Phase 7: Improve Discovery UX

Add read models and UI presentation for first seen, last seen, age, reported
measurement kinds, device diagnostics, assignment coverage, and ended
bindings. Make the stale threshold configurable at the application boundary.

Automatic replacement suggestions remain future work. If added later, model a
suggestion separately from a confirmed binding so inference can never silently
rewrite continuity.

Gate: deterministic read-model tests use an injected/current-time value rather
than wall-clock sleeps.

### Phase 8: Optional Module Reorganization

Only after the behavioral boundaries are stable, consider moving modules to:

```text
src/
  domain/
  application/
    ports/
    services/
  adapters/
    mqtt/
    http/
    prometheus/
    duckdb/
  server.rs
  service.rs
  orchestrator.rs
  shutdown_token.rs
```

This move is cosmetic. Perform it separately from behavior changes so review
history remains useful.

## Port Design Guidance

Likely input-port capabilities include:

- ingest a received payload;
- list/discover physical devices;
- create or rename logical sensors;
- create, close, and inspect series bindings;
- query physical and logical readings; and
- obtain latest logical readings for export.

Likely output-port capabilities include:

- atomically persist ingestion results;
- load/update the physical-device catalog and diagnostics;
- persist logical sensors and series bindings; and
- execute typed physical/logical read models.

These are directions, not a required one-trait-per-bullet layout. Define each
port immediately before migrating its first real caller, and give it both a
real adapter and a useful fake in tests.

Avoid returning `anyhow::Error` across every application boundary. Use typed
errors for expected outcomes such as validation failures, unknown IDs, and
binding conflicts. Reserve opaque errors for unexpected infrastructure faults.

## What to Preserve

- the single-owner DuckDB worker pattern behind `DbHandle`;
- source-before-sink shutdown so final ingestion can complete before DuckDB
  closes;
- retention of raw input;
- deterministic, bounded tests that need no external broker;
- in-memory DuckDB repository tests;
- small composition logic in `server.rs`; and
- incremental, shippable changes.

Preserving an abstraction does not mean freezing its current API. `DbHandle`
may gain typed commands or more general parameter support while remaining an
adapter-internal concurrency mechanism.

## Explicit Non-Goals

- automatically deciding that two reported identities are the same hardware;
- AI/ML pipelines;
- supporting arbitrary databases before a second adapter is genuinely wanted;
- event sourcing as an architectural project in itself;
- a generic IoT protocol framework;
- exposing every raw device field as a Prometheus label; and
- achieving a textbook directory structure at the expense of working slices.

## Completion Criteria

The migration is complete when:

- MQTT, HTTP, Prometheus, DuckDB, Arrow, and SQL do not leak into the domain;
- changing the ingestion adapter does not change normalization or persistence
  policy;
- a reported-ID change can be manually connected to an existing logical
  measurement series with explicit temporal behavior;
- partial device replacement works per measurement kind;
- conflicting assignments are rejected and explained;
- every payload survives parse, mapping, and storage-edge cases without silent
  buffer loss;
- Prometheus exports stable latest logical readings;
- physical-device discovery and staleness are visible;
- important application behavior is testable with fakes; and
- DuckDB integration is covered independently with real in-memory database
  tests.
