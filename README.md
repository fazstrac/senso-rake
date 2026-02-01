# WIP: SensoRake

<img src="img/sensorake-logo.png" alt="SensoRake logo">

This is an early-phase project to learn Rust and to create the foundations of a service that 
- [x] listens to MQTT messages (e.g., weather sensors)
- [x] persists raw data into a data store (DuckLake / DuckDB)
- [ ] exposes metrics and mappings for Prometheus scraping
- [ ] exposes an admin endpoint to allow creating and mapping from raw sensor identification data into logical name like `Bedroom temp sensor`

What this baseline will eventually provide
- An async HTTP server (`axum`)
	- A tiny web UI to administer sensor readings with `metrics` endpoint for Prometheus
- An MQTT listener stub (uses `rumqttc`) that subscribes to user-provided MQTT topic and increments a Prometheus counter for incoming messages. 
- DONE: persist raw messages into DuckDB

**Disclosure**: The baseline is created using Github Copilot Pro, as a project to get something done while learning Rust at the same time. Next steps include moving towards human-created and vetted code.

## Running locally
- Build and run the Rust server:
```bash
cargo build
cargo run
```
- The server listens on `http://127.0.0.1:3000/` by default. The UI is available at `/` and the Prometheus metrics at `/metrics`.

UI (development and build)
- Install dependencies (using yarn):
```bash
cd ui
yarn
```
- Start Vite dev server:
```bash
yarn dev
```
- Build static assets for production (outputs to `ui/dist`):
```bash
yarn build
```