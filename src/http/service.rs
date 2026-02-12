// HTTP handlers for the service. This module sets up the Axum
// router with routes, handlers, and middleware.
use crate::service::{Service, ServiceType};
use crate::shutdown_token::ShutdownToken;
use crate::database;

use log::{info, error};

use prometheus::{Encoder, Registry, TextEncoder};
use std::sync::Arc;

use axum::extract::Extension;
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, put, post};
use axum::Router;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct SensorMapping {
    model: String,
    id: String,
    description: String,
}



pub struct HttpService {
    // This is a placeholder for future database handle integration. To be implemented
    // during HTTP service refactor
    http_db_handle: database::DbHandle,
    prom_registry: Arc<Registry>,
    shutdown_token: ShutdownToken, // This should be replaced with a shutdown token
}

impl HttpService {
    pub fn new(
        // This is a placeholder for future database handle integration. To be implemented
        // during HTTP service refactor
        http_db_handle: database::DbHandle,
        prom_registry: Arc<Registry>,
        shutdown_token: ShutdownToken,
    ) -> Self {
        Self {
            http_db_handle,
            prom_registry,
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
            self.http_db_handle.clone(),
            self.prom_registry.clone(),
            self.shutdown_token.clone(),
        )
        .await
    }
}

pub async fn start_http_server(
    http_db_handle: database::DbHandle,
    prom_registry: Arc<Registry>,
    shutdown_token: ShutdownToken,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let app = build_router(http_db_handle, prom_registry);
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
    http_db_handle: database::DbHandle,
    prom_registry: Arc<Registry>,
) -> Router {
    Router::new()
        // These are subject to change as we refactor the HTTP service
        // .route("/mappings", put(put_mapping).get(list_mappings))
        .route("/temperatures", get(temperatures_get_handler))        
        .route("/pressures", get(pressures_get_handler))
        .route("/humidities", get(humidities_get_handler))
        .route("/mappings", get(mappings_get_handler))
        .route("/metrics", get(metrics_handler))
        .route("/health", get(|| async { StatusCode::OK }))
        .layer(Extension(prom_registry))
        .layer(Extension(http_db_handle))
        .layer(middleware::from_fn(cors_middleware))
}


async fn temperatures_get_handler(Extension(http_db_handle): Extension<database::DbHandle>,) -> Response {
    query_helper(http_db_handle, "SELECT * FROM latest_temperatures".to_string()).await
}

async fn pressures_get_handler(Extension(http_db_handle): Extension<database::DbHandle>) -> Response {
    query_helper(http_db_handle, "SELECT * FROM latest_pressures".to_string()).await
}

async fn humidities_get_handler(Extension(http_db_handle): Extension<database::DbHandle>) -> Response {
    query_helper(http_db_handle, "SELECT * FROM latest_humidities".to_string()).await
}

async fn mappings_get_handler(Extension(http_db_handle): Extension<database::DbHandle>) -> Response {
    query_helper(http_db_handle, "SELECT * FROM mappings WHERE deleted = false".to_string()).await
}



async fn query_helper(http_db_handle: database::DbHandle, query: String) -> Response {
    let res_json = http_db_handle.query(query).await;

    match res_json {
        Ok(db_response) => {
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(db_response.to_string()))
                .unwrap()
        },
        Err(msg) => {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::from(format!("Database query failed: {}", msg)))
                .unwrap()
        }
    }
}



/// Expose Prometheus text-format metrics gathered from the provided
/// `Registry` extension. This returns the body and an (empty) header map so
/// the caller can set the appropriate `Content-Type` if needed.
async fn metrics_handler(Extension(prom_registry): Extension<Arc<Registry>>) -> (HeaderMap, String) {
    let encoder = TextEncoder::new();
    let metric_families = prom_registry.gather();
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
        if let Some(origin) = req.headers().get("origin") {
            // In production, you might want to validate the origin here
            // against a whitelist before echoing it back.
            origin.clone()
        } else {
            // Fallback if no Origin header is present
            HeaderValue::from_static("https://your-frontend.example.com")
        }
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

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use axum::body::Body;
//     use axum::http::{Request, StatusCode};
//     use prometheus::Registry;
//     use std::sync::Arc;

//     use axum::Router;
//     use axum::http::Method;

//     use tower::util::ServiceExt; // for `oneshot` method

//     fn build_test_app() -> Router {
//         let registry = Arc::new(Registry::new());
        
//         // Create mock database handle
//         let (tx, rx) = crossbeam_channel::unbounded::<database::DbJob>();
//         let db = database::DbHandle::new(tx);
        
//         // Spawn minimal mock worker
//         std::thread::spawn(move || {
//             while let Ok(job) = rx.recv() {
//                 // Respond OK to all commands without actually doing work
//                 let _ = job.response.send(Ok(database::DbResponse::InsertResult));
//             }
//         });
        
//         build_router(db, registry)
//     }

//     #[tokio::test]
//     async fn health_endpoint_works() {
//         let app = build_test_app();

//         let response = app
//             .oneshot(
//                 Request::builder()
//                     .uri("/health")
//                     .method(Method::GET)
//                     .body(Body::empty())
//                     .unwrap(),
//             )
//             .await
//             .unwrap();

//         assert_eq!(response.status(), StatusCode::OK);
//     }

//     #[tokio::test]
//     async fn metrics_endpoint_works() {
//         let app = build_test_app();

//         let response = app
//             .oneshot(
//                 Request::builder()
//                     .uri("/metrics")
//                     .method(Method::GET)
//                     .body(Body::empty())
//                     .unwrap(),
//             )
//             .await
//             .unwrap();

//         assert_eq!(response.status(), StatusCode::OK);
//     }
// }
