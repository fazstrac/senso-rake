// `server.rs` composes the HTTP application: it loads initial state,
// registers Prometheus metrics, starts the MQTT background task, and
// mounts HTTP handlers and middleware.
use crate::{handlers, mqtt, db, signals, state::{load_mappings, Store}};
use axum::{routing::{get, put}, Router, Extension};
use prometheus::{Registry, IntCounter};
use std::sync::Arc;

use axum::middleware::{self, Next};
use axum::response::Response;
use axum::http::{Request, Method, HeaderValue, StatusCode};

// TODO
// - IDEA: reread config/mappings on SIGHUP?
// - Centralized database handler shared between MQTT task and HTTP handlers
// - Persist mappings to database

pub async fn run() -> anyhow::Result<()> {
    let initial = load_mappings().await.unwrap_or_default();
    let store: Store = Arc::new(tokio::sync::RwLock::new(initial));

    let registry = Arc::new(Registry::new());
    let mqtt_messages_received_counter = IntCounter::new("mqtt_messages_total", "Total MQTT messages received").unwrap();
    let mqtt_messages_not_flushed_to_db = IntCounter::new("mqtt_unflushed_total", "Total unflushed MQTT messages in WAL").unwrap();
    registry.register(Box::new(mqtt_messages_received_counter.clone())).ok();
    registry.register(Box::new(mqtt_messages_not_flushed_to_db.clone())).ok();



    // Start DB worker and pass handle into background tasks
    let mqtt_messages_not_flushed_to_db_handle = mqtt_messages_not_flushed_to_db.clone();
    let db_path = std::env::var("DUCKDB_PATH").ok();
    let (db_handle, db_join_handle) = db::start_db_worker(db_path, mqtt_messages_not_flushed_to_db_handle);

    let shutdown_notify = Arc::new(tokio::sync::Notify::new());
    let shutdown_notify_task = shutdown_notify.clone();



    // Start MQTT background task
    let mqtt_messages_received_counter_task = mqtt_messages_received_counter.clone();
    let mqtt_messages_not_flushed_to_db_task = mqtt_messages_not_flushed_to_db.clone();
    let db_for_task = db_handle.clone();
    let mqtt_join_handle = mqtt::start_mqtt_worker(
        mqtt_messages_received_counter_task, 
        mqtt_messages_not_flushed_to_db_task, 
        db_for_task, 
        shutdown_notify_task
    ).await.unwrap();


    // Spawn a task to handle Unix signals for graceful shutdown
    let shutdown_notify_task2 = shutdown_notify.clone();

    let db_handle_for_signal_task = db_handle.clone();

    let _signal_handler = signals::start_signal_handler(
        shutdown_notify_task2,
        mqtt_join_handle,
        db_handle_for_signal_task,
        db_join_handle,
    ).await;

    let http_db_handle = db_handle.clone();

    // Build the HTTP app. Layers are applied from bottom -> top: the
    // `Extension` layers provide shared state (Store and Registry) to
    // handlers. The CORS middleware is mounted last so it can ensure
    // headers are applied to all responses.
    let app = Router::new()
        .route("/mapping", put(handlers::put_mapping).get(handlers::list_mappings))
        .route("/metrics", get(handlers::metrics_handler))
        .route("/health", get(|| async { "ok" }))
        .fallback_service(get(handlers::spa_handler))
        .layer(Extension(store))
        .layer(Extension(registry))
        .layer(Extension(http_db_handle))
        .layer(middleware::from_fn(cors_middleware));

    let bind_addr = "0.0.0.0:3000";
    println!("listening on {}", bind_addr);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    let server = axum::serve(listener, app);

    let shutdown_future = {
        let shutdown_notify_task3 = shutdown_notify.clone();
        async move {
            shutdown_notify_task3.notified().await;
            println!("HTTP server shutdown signal received.");
        }
    };

    server.with_graceful_shutdown(shutdown_future).await?;
    // signal_task.await.unwrap();

    Ok(())
}

async fn cors_middleware(req: Request<axum::body::Body>, next: Next) -> Response {
    let allow_headers = HeaderValue::from_static("*");
    let allow_methods = HeaderValue::from_static("GET,PUT,POST,OPTIONS");
    let allow_origin = HeaderValue::from_static("*");

    if req.method() == &Method::OPTIONS {
        let mut res = Response::new(axum::body::Body::empty());
        *res.status_mut() = StatusCode::NO_CONTENT;
        let headers = res.headers_mut();
        // Common preflight response headers. For production, consider
        // restricting `allow_origin` to your frontend domain and only
        // allowing the specific headers you need.
        headers.insert("access-control-allow-origin", allow_origin.clone());
        headers.insert("access-control-allow-methods", allow_methods.clone());
        headers.insert("access-control-allow-headers", allow_headers.clone());
        return res;
    }

    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    // Attach the same CORS headers to the normal responses so the browser
    // accepts the responses from the API.
    headers.insert("access-control-allow-origin", allow_origin);
    headers.insert("access-control-allow-methods", allow_methods);
    headers.insert("access-control-allow-headers", allow_headers);
    res
}
