// HTTP handlers for the service. This module sets up the Axum
// router with routes, handlers, and middleware.
use crate::service::{Service, ServiceType};
use crate::shutdown_token::ShutdownToken;
use crate::state::{Mapping};

use log::{info, error};

use prometheus::{Encoder, Registry, TextEncoder};
use std::sync::Arc;

use axum::extract::Extension;
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, put};
use axum::{Json, Router};

pub struct HttpService {
    // This is a placeholder for future database handle integration. To be implemented
    // during HTTP service refactor
    // http_db_handle: db::DbHandle,
    registry: Arc<Registry>,
    shutdown_token: ShutdownToken, // This should be replaced with a shutdown token
}

impl HttpService {
    pub fn new(
        // This is a placeholder for future database handle integration. To be implemented
        // during HTTP service refactor
        // http_db_handle: db::DbHandle,
        registry: Arc<Registry>,
        shutdown_token: ShutdownToken,
    ) -> Self {
        Self {
            // http_db_handle,
            registry,
            shutdown_token,
        }
    }
}

#[async_trait::async_trait]
impl Service for HttpService {
    fn svc(&self) -> ServiceType {
        ServiceType::Source
    }

    async fn start(&self) -> anyhow::Result<tokio::task::JoinHandle<()>> {
        start_http_server(
            // This is a placeholder for future database handle integration. To be implemented
            // during HTTP service refactor
            // self.http_db_handle.clone(),
            self.registry.clone(),
            self.shutdown_token.clone(),
        )
        .await
    }
}

pub async fn start_http_server(
    // http_db_handle: db::DbHandle, // commented out for now to wait for refactor
    registry: Arc<Registry>,
    shutdown_token: ShutdownToken,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let app = build_router(registry);

    let bind_addr = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    let join_handle = tokio::spawn(async move {
        info!("listening on {}", bind_addr);

        let server = axum::serve(listener, app);

        let shutdown_future = {
            async move {
                shutdown_token.wait().await;
                info!("HTTP server shutdown signal received.");
            }
        };

        if let Err(err) = server
            .with_graceful_shutdown(shutdown_future)
            .await {
            error!("HTTP server in shutdown: {}", err);
        };
    });

    Ok(join_handle)
}

// Build the Axum router with routes, handlers, and middleware.
// This is separate function for testability.
fn build_router(
    // http_db_handle: db::DbHandle,
    registry: Arc<Registry>,
) -> Router {
    Router::new()
        // These are subject to change as we refactor the HTTP service
        .route("/mappings", put(put_mapping).get(list_mappings))
        .route("/metrics", get(metrics_handler))
        .route("/health", get(|| async { StatusCode::OK }))
        .layer(Extension(registry))
        // .layer(Extension(http_db_handle)) // commented out for now to wait for refactor
        .layer(middleware::from_fn(cors_middleware))
}

/// Return all mappings as a JSON array.
///
/// NOTE: This is currently a stub implementation and does not read from
/// real storage. It returns an empty list until storage integration is
/// wired in.
async fn list_mappings() -> Json<Vec<Mapping>> {
    // TODO: Integrate with persistent storage to fetch real mappings.
    Json(Vec::new())
}

/// Insert or update a mapping. Expects a JSON body matching `Mapping`.
/// 
/// NOTE: This endpoint is currently not connected to persistent storage and
/// will always return `501 Not Implemented`. It is intentionally left
/// non-functional until the database refactor is complete.
async fn put_mapping(Json(_payload): Json<Mapping>) -> (StatusCode, String) {
    (
        StatusCode::NOT_IMPLEMENTED,
        "PUT /mappings is not implemented: mapping storage is currently disabled"
            .to_string(),
    )
}

/// Expose Prometheus text-format metrics gathered from the provided
/// `Registry` extension. This returns the body and an (empty) header map so
/// the caller can set the appropriate `Content-Type` if needed.
async fn metrics_handler(Extension(registry): Extension<Arc<Registry>>) -> (HeaderMap, String) {
    let encoder = TextEncoder::new();
    let metric_families = registry.gather();
    let mut buffer = Vec::new();
    encoder
        .encode(&metric_families, &mut buffer)
        .unwrap_or_default();
    let body = String::from_utf8_lossy(&buffer).to_string();
    (HeaderMap::new(), body)
}

async fn cors_middleware(req: Request<axum::body::Body>, next: Next) -> Response {
    // In debug builds, keep permissive CORS for easier local development.
    // In release (production) builds, restrict origins and headers to reduce
    // the risk of CSRF and cross-origin data exfiltration.
    let allow_headers = if cfg!(debug_assertions) {
        HeaderValue::from_static("*")
    } else {
        // Adjust this list to the specific headers your frontend actually needs.
        HeaderValue::from_static("Content-Type, Authorization")
    };
    let allow_methods = HeaderValue::from_static("GET,PUT,POST,OPTIONS");
    let allow_origin = if cfg!(debug_assertions) {
        HeaderValue::from_static("*")
    } else {
        // Replace this with your actual frontend origin, e.g.:
        // HeaderValue::from_static("https://app.example.com")
        HeaderValue::from_static("https://your-frontend.example.com")
    };

    if req.method() == Method::OPTIONS {
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use prometheus::Registry;
    use std::sync::Arc;

    use axum::Router;
    use axum::http::Method;

    use tower::util::ServiceExt; // for `oneshot` method

    fn build_test_app() -> Router {
        let registry = Arc::new(Registry::new());
        // let db: db::DbHandle;

        build_router(registry)
    }

    #[tokio::test]
    async fn health_endpoint_works() {
        let app = build_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .method(Method::GET)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn metrics_endpoint_works() {
        let app = build_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .method(Method::GET)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
