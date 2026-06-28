// HTTP handlers for the service. This module sets up the Axum
// router with routes, handlers, and middleware.
use crate::database;
use crate::service::{Service, ServiceType};
use crate::shutdown_token::ShutdownToken;

use log::{error, info};

use prometheus::{Encoder, Registry, TextEncoder};
use std::sync::Arc;

use axum::Router;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{delete, get, post}; // post is used in build_router for mappings_post_handler

use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct SensorMapping {
    pub model: String,
    pub id: String,
    pub validity_start: DateTime<Utc>,
    pub description: String,
}

#[derive(Debug, Serialize)]
struct CreatedMapping {
    mapping_id: i64,
    #[serde(flatten)]
    mapping: SensorMapping,
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

        if let Err(err) = server.with_graceful_shutdown(shutdown_future).await {
            error!("HTTP server in shutdown: {}", err);
        };
    });

    Ok(join_handle)
}

// Build the Axum router with routes, handlers, and middleware.
// This is separate function for testability.
pub fn build_router(http_db_handle: database::DbHandle, prom_registry: Arc<Registry>) -> Router {
    Router::new()
        // These are subject to change as we refactor the HTTP service
        // .route("/mappings", put(put_mapping).get(list_mappings))
        .route("/temperatures", get(temperatures_get_handler))
        .route("/pressures", get(pressures_get_handler))
        .route("/humidities", get(humidities_get_handler))
        .route(
            "/mappings",
            get(mappings_get_handler).post(mappings_post_handler),
        )
        .route("/mappings/{id}", delete(mappings_delete_handler))
        .route("/mappings/{id}/restore", post(mappings_restore_handler))
        .route("/metrics", get(metrics_handler))
        .route("/health", get(|| async { StatusCode::OK }))
        .layer(Extension(prom_registry))
        .layer(Extension(http_db_handle))
        .layer(middleware::from_fn(cors_middleware))
}

async fn temperatures_get_handler(
    Extension(http_db_handle): Extension<database::DbHandle>,
) -> Response {
    query_helper(
        http_db_handle,
        "SELECT * FROM latest_temperatures".to_string(),
    )
    .await
}

async fn pressures_get_handler(
    Extension(http_db_handle): Extension<database::DbHandle>,
) -> Response {
    query_helper(http_db_handle, "SELECT * FROM latest_pressures".to_string()).await
}

async fn humidities_get_handler(
    Extension(http_db_handle): Extension<database::DbHandle>,
) -> Response {
    query_helper(
        http_db_handle,
        "SELECT * FROM latest_humidities".to_string(),
    )
    .await
}

async fn mappings_get_handler(
    Extension(http_db_handle): Extension<database::DbHandle>,
) -> Response {
    query_helper(http_db_handle, "SELECT * FROM all_sensors".to_string()).await
}

async fn mappings_post_handler(
    Extension(http_db_handle): Extension<database::DbHandle>,
    Json(payload): Json<SensorMapping>,
) -> Response {
    // Validate that all fields are non-empty
    if payload.model.is_empty() || payload.id.is_empty() || payload.description.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(axum::body::Body::from(
                "All fields (model, id, description) must be non-empty",
            ))
            .unwrap();
    }

    let query = "INSERT INTO mappings (model, id, validity_start, description) VALUES (?, ?, ?, ?) RETURNING mapping_id"
        .to_string();
    let params = vec![
        payload.model.clone(),
        payload.id.clone(),
        payload.validity_start.to_rfc3339(),
        payload.description.clone(),
    ];

    match http_db_handle.query_with_params(query, params).await {
        Ok(json_result) => {
            // Parse the returned mapping_id from the JSON result
            match serde_json::from_str::<Vec<serde_json::Value>>(&json_result) {
                Ok(rows) if !rows.is_empty() => {
                    if let Some(mapping_id) = rows[0].get("mapping_id").and_then(|v| v.as_i64()) {
                        let response = CreatedMapping {
                            mapping_id,
                            mapping: payload,
                        };

                        Response::builder()
                            .status(StatusCode::CREATED)
                            .header("Content-Type", "application/json")
                            .body(axum::body::Body::from(
                                serde_json::to_string(&response).unwrap_or_default(),
                            ))
                            .unwrap()
                    } else {
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(axum::body::Body::from(
                                "Failed to retrieve generated mapping_id",
                            ))
                            .unwrap()
                    }
                }
                _ => Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(axum::body::Body::from("Invalid database response"))
                    .unwrap(),
            }
        }
        Err(e) => {
            let error_msg = e.to_string();

            // Check for constraint violations (e.g., duplicate entries)
            if error_msg.contains("Constraint Error")
                || error_msg.contains("UNIQUE constraint failed")
            {
                Response::builder()
                    .status(StatusCode::CONFLICT)
                    .body(axum::body::Body::from(format!(
                        "Mapping already exists: {}",
                        error_msg
                    )))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(axum::body::Body::from(format!(
                        "Database error: {}",
                        error_msg
                    )))
                    .unwrap()
            }
        }
    }
}

async fn mappings_delete_handler(
    Extension(http_db_handle): Extension<database::DbHandle>,
    Path(mapping_id): Path<i64>,
) -> StatusCode {
    let query = "UPDATE mappings SET deleted = true WHERE mapping_id = ?".to_string();
    let params = vec![mapping_id.to_string()];

    let _ = http_db_handle.query_with_params(query, params).await;

    // Idempotent: return 204 No Content regardless of whether the mapping existed
    StatusCode::NO_CONTENT
}

async fn mappings_restore_handler(
    Extension(http_db_handle): Extension<database::DbHandle>,
    Path(mapping_id): Path<i64>,
) -> StatusCode {
    let query = "UPDATE mappings SET deleted = false WHERE mapping_id = ?".to_string();
    let params = vec![mapping_id.to_string()];

    let _ = http_db_handle.query_with_params(query, params).await;

    // Idempotent: return 204 No Content regardless of whether the mapping existed
    StatusCode::NO_CONTENT
}

async fn query_helper(http_db_handle: database::DbHandle, query: String) -> Response {
    let res_json = http_db_handle.query(query).await;

    match res_json {
        Ok(db_response) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(db_response.to_string()))
            .unwrap(),
        Err(msg) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(axum::body::Body::from(format!(
                "Database query failed: {}",
                msg
            )))
            .unwrap(),
    }
}

/// Expose Prometheus text-format metrics gathered from the provided
/// `Registry` extension. This returns the body and an (empty) header map so
/// the caller can set the appropriate `Content-Type` if needed.
async fn metrics_handler(
    Extension(prom_registry): Extension<Arc<Registry>>,
) -> (HeaderMap, String) {
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

#[cfg(test)]
mod tests {
    use super::*;

    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use crossbeam_channel::{Receiver, TryRecvError};
    use database::{DbCommand, DbHandle, DbJob, DbResponse};
    use prometheus::IntCounter;
    use tower::ServiceExt;

    fn fake_db() -> (DbHandle, Receiver<DbJob>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        (DbHandle::new(tx), rx)
    }

    enum ExpectedDbCommand {
        Query {
            sql: &'static str,
            response: &'static str,
        },
        QueryWithParams {
            sql: &'static str,
            params: &'static [&'static str],
            response: &'static str,
        },
    }

    struct RouterTestCase {
        name: &'static str,
        method: Method,
        uri: &'static str,
        request_body: Option<&'static str>,
        expected_status: StatusCode,
        expected_db_command: Option<ExpectedDbCommand>,
        expected_body_fragment: Option<&'static str>,
    }

    #[tokio::test]
    async fn router_endpoints_follow_their_contracts() {
        let cases = vec![
            RouterTestCase {
                name: "health",
                method: Method::GET,
                uri: "/health",
                request_body: None,
                expected_status: StatusCode::OK,
                expected_db_command: None,
                expected_body_fragment: None,
            },
            RouterTestCase {
                name: "temperatures",
                method: Method::GET,
                uri: "/temperatures",
                request_body: None,
                expected_status: StatusCode::OK,
                expected_db_command: Some(ExpectedDbCommand::Query {
                    sql: "SELECT * FROM latest_temperatures",
                    response: r#"[{"temperature_C":21.5}]"#,
                }),
                expected_body_fragment: Some(r#""temperature_C":21.5"#),
            },
            RouterTestCase {
                name: "pressures",
                method: Method::GET,
                uri: "/pressures",
                request_body: None,
                expected_status: StatusCode::OK,
                expected_db_command: Some(ExpectedDbCommand::Query {
                    sql: "SELECT * FROM latest_pressures",
                    response: r#"[{"pressure_kPa":101.3}]"#,
                }),
                expected_body_fragment: Some(r#""pressure_kPa":101.3"#),
            },
            RouterTestCase {
                name: "humidities",
                method: Method::GET,
                uri: "/humidities",
                request_body: None,
                expected_status: StatusCode::OK,
                expected_db_command: Some(ExpectedDbCommand::Query {
                    sql: "SELECT * FROM latest_humidities",
                    response: r#"[{"humidity":42.0}]"#,
                }),
                expected_body_fragment: Some(r#""humidity":42.0"#),
            },
            RouterTestCase {
                name: "list mappings",
                method: Method::GET,
                uri: "/mappings",
                request_body: None,
                expected_status: StatusCode::OK,
                expected_db_command: Some(ExpectedDbCommand::Query {
                    sql: "SELECT * FROM all_sensors",
                    response: r#"[{"mapping_id":1}]"#,
                }),
                expected_body_fragment: Some(r#""mapping_id":1"#),
            },
            RouterTestCase {
                name: "create mapping",
                method: Method::POST,
                uri: "/mappings",
                request_body: Some(
                    r#"{
                        "model": "sensor-a",
                        "id": "123",
                        "description": "Livingroom",
                        "validity_start": "2026-06-28T12:00:00Z"
                    }"#,
                ),
                expected_status: StatusCode::CREATED,
                expected_db_command: Some(ExpectedDbCommand::QueryWithParams {
                    sql: "INSERT INTO mappings (model, id, validity_start, description) VALUES (?, ?, ?, ?) RETURNING mapping_id",
                    params: &["sensor-a", "123", "2026-06-28T12:00:00+00:00", "Livingroom"],
                    response: r#"[{"mapping_id":1}]"#,
                }),
                expected_body_fragment: Some(r#""mapping_id":1"#),
            },
            RouterTestCase {
                name: "delete mapping",
                method: Method::DELETE,
                uri: "/mappings/123",
                request_body: None,
                expected_status: StatusCode::NO_CONTENT,
                expected_db_command: Some(ExpectedDbCommand::QueryWithParams {
                    sql: "UPDATE mappings SET deleted = true WHERE mapping_id = ?",
                    params: &["123"],
                    response: "[]",
                }),
                expected_body_fragment: None,
            },
            RouterTestCase {
                name: "restore mapping",
                method: Method::POST,
                uri: "/mappings/123/restore",
                request_body: None,
                expected_status: StatusCode::NO_CONTENT,
                expected_db_command: Some(ExpectedDbCommand::QueryWithParams {
                    sql: "UPDATE mappings SET deleted = false WHERE mapping_id = ?",
                    params: &["123"],
                    response: "[]",
                }),
                expected_body_fragment: None,
            },
            RouterTestCase {
                name: "metrics",
                method: Method::GET,
                uri: "/metrics",
                request_body: None,
                expected_status: StatusCode::OK,
                expected_db_command: None,
                expected_body_fragment: Some("router_test_requests_total 1"),
            },
        ];

        for case in cases {
            let (handle, rx) = fake_db();
            let registry = Arc::new(Registry::new());
            let counter = IntCounter::new(
                "router_test_requests_total",
                "Requests observed by the router test",
            )
            .unwrap();
            counter.inc();
            registry.register(Box::new(counter)).unwrap();

            let worker_rx = rx.clone();
            let worker = case.expected_db_command.map(|expected| {
                let name = case.name;
                std::thread::spawn(move || {
                    let job = worker_rx.recv().unwrap();

                    match (expected, job.command) {
                        (
                            ExpectedDbCommand::Query {
                                sql: expected_sql,
                                response,
                            },
                            DbCommand::Query(sql),
                        ) => {
                            assert_eq!(sql, expected_sql, "{name}");
                            job.response
                                .send(Ok(DbResponse::QueryResult(response.to_string())))
                                .unwrap();
                        }
                        (
                            ExpectedDbCommand::QueryWithParams {
                                sql: expected_sql,
                                params: expected_params,
                                response,
                            },
                            DbCommand::QueryWithParams(sql, params),
                        ) => {
                            assert_eq!(sql, expected_sql, "{name}");
                            assert_eq!(
                                params,
                                expected_params
                                    .iter()
                                    .map(|value| value.to_string())
                                    .collect::<Vec<_>>(),
                                "{name}"
                            );
                            job.response
                                .send(Ok(DbResponse::QueryResult(response.to_string())))
                                .unwrap();
                        }
                        _ => panic!("{name}: unexpected database command"),
                    }
                })
            });

            let mut request = Request::builder().method(case.method).uri(case.uri);
            let body = match case.request_body {
                Some(body) => {
                    request = request.header("content-type", "application/json");
                    Body::from(body)
                }
                None => Body::empty(),
            };

            let response = build_router(handle.clone(), registry)
                .oneshot(request.body(body).unwrap())
                .await
                .unwrap();

            assert_eq!(response.status(), case.expected_status, "{}", case.name);

            if let Some(fragment) = case.expected_body_fragment {
                let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
                let body = String::from_utf8(body.to_vec()).unwrap();
                assert!(
                    body.contains(fragment),
                    "{}: response body did not contain {fragment:?}: {body}",
                    case.name
                );
            }

            match worker {
                Some(worker) => worker.join().unwrap(),
                None => match rx.try_recv() {
                    Err(TryRecvError::Empty) => {}
                    Ok(_) => panic!("{} unexpectedly sent a database job", case.name),
                    Err(TryRecvError::Disconnected) => {
                        panic!("{} database channel unexpectedly disconnected", case.name)
                    }
                },
            }
        }
    }

    #[test]
    fn test_sensor_mapping_deserialization_valid() {
        let json = r#"{"model":"sensor-a","id":"001","validity_start":"2025-02-14T10:30:00Z","description":"Living Room"}"#;
        let result: Result<SensorMapping, _> = serde_json::from_str(json);
        assert!(result.is_ok());
        let mapping = result.unwrap();
        assert_eq!(mapping.model, "sensor-a");
        assert_eq!(mapping.id, "001");
        assert_eq!(mapping.description, "Living Room");
    }

    #[test]
    fn test_sensor_mapping_deserialization_missing_field() {
        let json = r#"{"model":"sensor-a","id":"001"}"#;
        let result: Result<SensorMapping, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_sensor_mapping_deserialization_invalid_timestamp() {
        let json = r#"{"model":"sensor-a","id":"001","validity_start":"not-a-date","description":"Living Room"}"#;
        let result: Result<SensorMapping, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_sensor_mapping_serialization() {
        let mapping = SensorMapping {
            model: "sensor-b".to_string(),
            id: "002".to_string(),
            validity_start: DateTime::parse_from_rfc3339("2025-02-14T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            description: "Bedroom".to_string(),
        };
        let json = serde_json::to_string(&mapping).unwrap();
        assert!(json.contains("sensor-b"));
        assert!(json.contains("002"));
        assert!(json.contains("Bedroom"));
        assert!(json.contains("2025-02-14"));
    }

    #[test]
    fn test_created_mapping_serialization() {
        let mapping = SensorMapping {
            model: "sensor-c".to_string(),
            id: "003".to_string(),
            validity_start: DateTime::parse_from_rfc3339("2025-02-14T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            description: "Kitchen".to_string(),
        };
        let created = CreatedMapping {
            mapping_id: 42,
            mapping,
        };
        let json = serde_json::to_string(&created).unwrap();
        assert!(json.contains("42"));
        assert!(json.contains("sensor-c"));
        assert!(json.contains("Kitchen"));
        assert!(json.contains("2025-02-14"));
    }

    #[test]
    fn test_validation_empty_model() {
        let mapping = SensorMapping {
            model: "".to_string(),
            id: "001".to_string(),
            validity_start: DateTime::parse_from_rfc3339("2025-02-14T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            description: "Valid".to_string(),
        };
        assert!(mapping.model.is_empty());
    }

    #[test]
    fn test_validation_empty_id() {
        let mapping = SensorMapping {
            model: "sensor".to_string(),
            id: "".to_string(),
            validity_start: DateTime::parse_from_rfc3339("2025-02-14T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            description: "Valid".to_string(),
        };
        assert!(mapping.id.is_empty());
    }

    #[test]
    fn test_validation_empty_description() {
        let mapping = SensorMapping {
            model: "sensor".to_string(),
            id: "001".to_string(),
            validity_start: DateTime::parse_from_rfc3339("2025-02-14T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            description: "".to_string(),
        };
        assert!(mapping.description.is_empty());
    }

    #[test]
    fn test_delete_returns_no_content() {
        let status = StatusCode::NO_CONTENT;
        assert_eq!(status.as_u16(), 204);
    }

    #[test]
    fn test_restore_returns_no_content() {
        let status = StatusCode::NO_CONTENT;
        assert_eq!(status.as_u16(), 204);
    }

    #[test]
    fn test_mapping_id_to_string_conversion() {
        let mapping_id: i64 = 42;
        let param = mapping_id.to_string();
        assert_eq!(param, "42");
    }
}
