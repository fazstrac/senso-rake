// HTTP handlers for the service. These are thin wrappers around the shared
// `Store` and the Prometheus `Registry`. They intentionally do minimal
// validation to keep the example concise — add validation as needed.
use crate::{state::{key_for, save_mappings, Mapping, Store}, db};
use axum::{body::Body, extract::Extension, http::{HeaderMap, Request, StatusCode, header::CONTENT_TYPE, HeaderValue}, response::IntoResponse, Json};
use prometheus::{Encoder, Registry, TextEncoder};
use std::sync::Arc;

use axum::{routing::{get, put}, Router};

use axum::middleware::{self, Next};
use axum::response::Response;
use axum::http::{Method,};


pub async fn start_http_server(
    http_db_handle: db::DbHandle, 
    store: Store, 
    registry: Arc<Registry>, 
    shutdown_notify: Arc<tokio::sync::Notify>) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    // Build the HTTP app. Layers are applied from bottom -> top: the
    // `Extension` layers provide shared state (Store and Registry) to
    // handlers. The CORS middleware is mounted last so it can ensure
    // headers are applied to all responses.
    let app = Router::new()
        .route("/mapping", put(put_mapping).get(list_mappings))
        .route("/metrics", get(metrics_handler))
        .route("/health", get(|| async { "ok" }))
        .fallback_service(get(spa_handler))
        .layer(Extension(store))
        .layer(Extension(registry))
        .layer(Extension(http_db_handle))
        .layer(middleware::from_fn(cors_middleware));

    let bind_addr = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    let join_handle = tokio::spawn(async move {
        println!("listening on {}", bind_addr);

        let server = axum::serve(listener, app);

        let shutdown_future = {
            let shutdown_notify_task3 = shutdown_notify.clone();
            async move {
                shutdown_notify_task3.notified().await;
                println!("HTTP server shutdown signal received.");
            }
        };

        server.with_graceful_shutdown(shutdown_future).await.unwrap();
    });

    Ok(join_handle)
}


/// Return all mappings as JSON array. This performs a read-lock and clones the
/// values so the handler does not keep the lock across await points.
async fn list_mappings(Extension(store): Extension<Store>) -> Json<Vec<Mapping>> {
    let map = store.read().await;
    let vec = map.values().cloned().collect();
    Json(vec)
}

/// Insert or update a mapping. Expects a JSON body matching `Mapping`.
/// Returns `201 Created` on success. In a production service you'd validate
/// fields and possibly return `400 Bad Request` for invalid payloads.
async fn put_mapping(Extension(store): Extension<Store>, Json(payload): Json<Mapping>) -> Result<StatusCode, (StatusCode, String)> {
    let key = key_for(&payload.sensor_id, &payload.manufacturer);
    {
        let mut map = store.write().await;
        map.insert(key, payload);
    }
    // Persist immediately for this simple example. Consider batching in
    // high-throughput scenarios or moving persistence to a DB.
    save_mappings(&store).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::CREATED)
}

/// Expose Prometheus text-format metrics gathered from the provided
/// `Registry` extension. This returns the body and an (empty) header map so
/// the caller can set the appropriate `Content-Type` if needed.
async fn metrics_handler(Extension(registry): Extension<Arc<Registry>>) -> (HeaderMap, String) {
    let encoder = TextEncoder::new();
    let metric_families = registry.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap_or_default();
    let body = String::from_utf8_lossy(&buffer).to_string();
    (HeaderMap::new(), body)
}

/// Serve the built single-page app under `ui/dist`. The handler maps `/` to
/// `ui/dist/index.html` and otherwise attempts to read the requested file.
/// This is intentionally small — for production you might use a static file
/// server or embed assets in the binary.
async fn spa_handler(req: Request<Body>) -> impl IntoResponse {
    let path = req.uri().path();
    let rel = if path == "/" { "ui/dist/index.html".to_string() } else { format!("ui/dist{}", path) };

    match tokio::fs::read(&rel).await {
        Ok(bytes) => {
            let content_type = if rel.ends_with(".html") {
                "text/html; charset=utf-8"
            } else if rel.ends_with(".js") {
                "application/javascript; charset=utf-8"
            } else if rel.ends_with(".css") {
                "text/css; charset=utf-8"
            } else if rel.ends_with(".json") {
                "application/json; charset=utf-8"
            } else if rel.ends_with(".wasm") {
                "application/wasm"
            } else {
                "application/octet-stream"
            };
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
            (headers, bytes).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
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