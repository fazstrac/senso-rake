// `server.rs` composes the HTTP application: it loads initial state,
// registers Prometheus metrics, starts the MQTT background task, and
// mounts HTTP handlers and middleware.
use crate::{http_server, mqtt, db, signals, state::{load_mappings, Store}};
use prometheus::{Registry, IntCounter};
use std::sync::Arc;

// TODO
// [ ] reread config/mappings on SIGHUP?
// [x] Centralized database handler shared between MQTT task and HTTP handlers
// [ ] Persist mappings to database
pub async fn run() -> anyhow::Result<()> {
    let initial = load_mappings().await.unwrap_or_default();
    let store: Store = Arc::new(tokio::sync::RwLock::new(initial));

    let registry = Arc::new(Registry::new());
    let mqtt_messages_received_counter = IntCounter::new("mqtt_messages_total", "Total MQTT messages received").unwrap();
    let mqtt_messages_not_flushed_to_db = IntCounter::new("mqtt_unflushed_total", "Total unflushed MQTT messages in WAL").unwrap();
    registry.register(Box::new(mqtt_messages_received_counter.clone())).ok();
    registry.register(Box::new(mqtt_messages_not_flushed_to_db.clone())).ok();

    // Start DB worker and pass handle into background tasks
    let db_path = std::env::var("DUCKDB_PATH").ok();
    let (db_handle, db_join_handle) = db::start_db_worker(db_path);

    // Start MQTT worker
    let shutdown_notify = Arc::new(tokio::sync::Notify::new());
    let shutdown_notify_for_mqtt_task = shutdown_notify.clone();
    let mqtt_messages_received_counter_task = mqtt_messages_received_counter.clone();
    let mqtt_messages_not_flushed_to_db_task = mqtt_messages_not_flushed_to_db.clone();
    let db_for_task = db_handle.clone();
    let mqtt_join_handle = mqtt::start_mqtt_worker(
        mqtt_messages_received_counter_task, 
        mqtt_messages_not_flushed_to_db_task, 
        db_for_task, 
        shutdown_notify_for_mqtt_task
    ).await.unwrap();

    // Start HTTP server
    let shutdown_notify_task_for_http_task = shutdown_notify.clone();
    let http_db_handle = db_handle.clone();
    let http_join_handle = http_server::start_http_server(
        http_db_handle, 
        store.clone(), 
        registry.clone(), 
        shutdown_notify_task_for_http_task
    ).await.unwrap();

    // Start signals handler to handle signals for graceful shutdown
    // Signals handler will take ownership of shutdown_notify, database and join handles
    // TODO: refactor logic to better reflect that signals handler should orchestrate the workers
    let signal_handler = signals::start_signal_handler(
        shutdown_notify,
        mqtt_join_handle,
        db_handle,
        db_join_handle,
        http_join_handle,
    ).await;

    signal_handler.await.unwrap();

    Ok(())
}