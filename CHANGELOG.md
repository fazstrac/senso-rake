# Changelog

## [Unreleased] - feature/database-to-html-ui

### Overview

This feature branch introduces a complete web UI and REST API for managing sensor mappings in SensoRake. Previously, sensor mappings were not configurable via the service; this update enables operators to describe and manage sensors through an intuitive web interface, critical for normalizing raw MQTT sensor data into actionable information.

### What Was Implemented

#### **1. REST API for Sensor Mappings**
A complete RESTful API for managing sensor-to-description mappings with support for soft-deletes and historical tracking.

- **GET `/mappings`** — Retrieve all sensors with their mapping information, including unmapped sensors and soft-deleted mappings for restoration
- **POST `/mappings`** — Create new sensor mappings with required fields: model, sensor ID, validity start timestamp, and description
- **DELETE `/mappings/{id}`** — Soft-delete a mapping (mark as deleted without removing from database)
- **POST `/mappings/{id}/restore`** — Restore a soft-deleted mapping

**Why:** Provides operational control over sensor data and enables the normalization of raw MQTT sensor messages into meaningful, human-readable descriptions. Operators can now assign descriptions like "Living Room Temperature" to sensor IDs in seconds via the UI instead of editing the database directly.

#### **2. Minimal Web UI for Sensor Management**
A vanilla TypeScript + Vite-based web UI (no framework overhead) for managing sensor mappings.

**Features:**
- **Sensor List View** — Groups sensors by state:
  - "Active Mappings" — Sensors with descriptions (includes delete buttons)
  - "Unmapped Sensors" — Raw MQTT sensors awaiting description (clickable to pre-fill form)
  - "Deleted Mappings" — Soft-deleted mappings with restore buttons
- **Create Mapping Form** — Quick form to assign descriptions to sensors
  - Pre-fills sensor model and ID when clicking unmapped sensors for fast mapping workflows
  - Validates all required fields before submission
  - Auto-converts local time to ISO 8601 UTC format
- **Interactive Workflow** — Seamless create → assign → restore cycle; all changes immediately reflected in UI

**Why:** Provides a user-friendly interface for operators to manage the mapping lifecycle without database access. The unmapped sensor clickability dramatically speeds up the common workflow of discovering new sensors and assigning descriptions.

#### **3. Database Schema & Tracking**
Support for temporal and audit requirements in sensor mappings.

- **`validity_start` timestamp** — Tracks when each mapping became valid, enabling support for sensor migration/replacement scenarios. When a sensor is replaced, a new mapping can be created with a new validity_start date, preserving the historical record of which description applied during which time period.
- **`deleted` flag** — Soft-delete pattern preserving records for auditing and recovery
- **Updated views** — Latest sensor views now correctly group by description and account for mapping changes over time

**Why:**
- **Validity Start:** Supports sensor hardware replacement and equipment upgrades; operators can track that "SensorID-001" was "Front Door" until 2025-02-01, then became "Back Door" after replacement—enabling accurate historical analysis and auditing.
- **Soft Deletes:** Ensures compliance, preserves audit trails, and allows accidental deletion recovery without losing historical sensor data.

#### **4. Database Query Serialization**
Database queries now return JSON-formatted results instead of raw Arrow RecordBatches.

- Queries executed in the database worker thread are converted to JSON via the `arrow-json` crate
- HTTP handlers receive pre-serialized JSON strings, simplifying the API layer
- Reduces formatting logic overhead and keeps database concerns centralized

**Why:** Simplifies the HTTP layer and ensures consistent, efficient data formatting at the database boundary—improving performance and maintainability.

#### **5. Testing & Documentation**
- **Integration Tests** — HTTP mappings endpoint validation (`tests/http_mappings_integration.rs`)
- **UI Unit Tests** — 25+ tests covering sensor state detection, form validation, component rendering, and pre-fill logic
- **CLAUDE.md** — Comprehensive project guidance including architecture overview, HTTP API reference, UI component patterns, testing guidelines, and key implementation patterns
- **Taskfile.yml** — Build automation for common workflows (UI build/dev/test, full test suite)

**Why:** Ensures reliability of the new mappings system, enables future development with clear patterns, and provides operators a complete reference for extending the service.

### Commits

#### Feature: REST API & Mappings Infrastructure
- **`e9ffcdf`** refactor: integrate database handle into HttpService
- **`582a368`** feat: implement query command in DbService and return JSON results
- **`e196eaa`** fix: include db_handle in HTTP service
- **`36f9909`** feat: add support for database query with parameters
- **`58c68d7`** feat: add /mappings endpoint with GET, POST and DELETE verbs; added struct SensorMapping for validation
- **`79df9a3`** feat: enhance sensor mapping with validity start and add restore endpoint
- **`2560359`** feat: add integration tests for HTTP mappings payload validation

#### Feature: Database Schema & Views
- **`c4d22c0`** feat: include validity_start and deleted fields in mappings join for all_sensors view in preparation for UI development
- **`bd754a3`** feat: update latest sensor views to group by description to correctly account for changes in mappings

#### Feature: Web UI
- **`20d76a3`** feat: add first draft of a HTML + JS UI
  - Vanilla TypeScript + Vite setup
  - SensorListUI component (grouping by state, rendering)
  - CreateMappingFormUI component (form validation, pre-fill)
  - ApiClient module for backend communication
  - Type definitions and initial styling

#### Feature: Build & Development Tooling
- **`d7911f1`** feat: add Taskfile for UI build, development, and testing tasks

#### Chore: Maintenance & Documentation
- **`057b5b7`** chore: add CLAUDE.md for project guidance and essential commands
- **`f7b3ea6`** chore: remove `state.rs` as outdated
- **`2111c37`** chore: add initial steps to support adding mappings from description to sensor
- **`5ded034`** update: update dependencies in Cargo.toml and Cargo.lock
  - Upgraded duckdb to 1.4.4
  - Updated http dependency to 1.4
  - Added arrow-json 56.2.0 for query serialization
- **`e76b886`** chore: update CLAUDE.md with current state
- **`5677065`** chore: cargo fmt and cargo clippy make happy

### Breaking Changes
None. This branch is purely additive—existing MQTT → Database → Prometheus metrics pipeline remains unchanged.

### Migration Notes
- The HTTP service now requires database handle integration (completed in this branch)
- All sensor mapping timestamps must be provided in ISO 8601 UTC format (e.g., `2025-02-14T10:00:00Z`)
- Soft-deleted mappings are included in GET /mappings responses; UI filters and displays them separately

### Testing
- Run all tests (Rust + UI): `cargo test && task test`
- Run UI tests: `task ui:test` or `cd ui && yarn test`
- Run UI dev server: `task dev` or `cd ui && yarn dev`
- Run integration tests: `cargo test --test http_mappings_integration`

### Next Steps / Known Limitations
- Frontend currently uses jsdom for testing; full E2E tests with real browser automation would benefit future releases
- UI styling uses design tokens from logo; enhancements to theming/accessibility welcome in future iterations
- Sensor state detection (mapped/unmapped/deleted) is currently client-side; server-side aggregation could optimize performance for large sensor fleets
