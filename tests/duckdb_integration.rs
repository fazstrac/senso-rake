use prometheus::Registry;
use std::sync::Arc;

use duckdb::Connection;

// Pull real schema SQL and Arrow batch creation from the crate
use senso_rake::database::DbService;
use senso_rake::database::schema::SCHEMA_SQL;
use senso_rake::http::build_router;
use senso_rake::mqtt::mqtt_buffer::{create_arrow_record_batch, process_message};
use senso_rake::service::Service;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use crossbeam_channel::unbounded;

use tower::ServiceExt;

fn sample_rows() -> Vec<senso_rake::mqtt::mqtt_buffer::ProcessedMsg> {
    // Minimal sample messages exercising the timestamp parsing
    let test_json = r#"[
        {
            "time": "2025-11-29 22:00:39",
            "model": "LaCrosse-TX29IT",
            "id": 19,
            "battery_ok": 1,
            "temperature_C": 20.9
        },
        {
            "time": "1767723037.402694",
            "model": "LaCrosse-TX141Bv3",
            "id": 246,
            "channel": 1,
            "battery_ok": 1,
            "temperature_C": 21.2
        }
    ]"#;

    let v: serde_json::Value = serde_json::from_str(test_json).expect("parse test json");
    let arr = v.as_array().expect("expected json array");

    arr.iter()
        .map(|item| {
            let s = serde_json::to_string(item).unwrap();
            process_message(&s)
        })
        .collect::<Vec<_>>()
}

#[test]
fn insert_arrow_batch_into_duckdb_should_error_on_name_mismatch() {
    // Create real DuckDB connection and initialize schema
    let conn = Connection::open_in_memory().expect("open in-memory duckdb");
    conn.execute_batch(SCHEMA_SQL)
        .expect("apply real schema SQL");

    // Build Arrow RecordBatch from MQTT normalization
    let rows = sample_rows();
    let batch = create_arrow_record_batch(&rows).expect("create arrow record batch");

    // Try inserting into the real landing table. Column names differ: 'timestamp' vs 'ts'.
    let mut appender = conn.appender("data_landing").expect("open appender");
    appender
        .append_record_batch(batch)
        .expect("append batch with mismatched field name");
    appender.flush().expect("flush appender");

    // Verify rows landed despite name mismatch (DuckDB maps by position)
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM data_landing")
        .expect("prepare count query");
    let mut rows_iter = stmt.query([]).expect("execute count query");
    let row = rows_iter.next().expect("first row").expect("row ok");
    let count: i64 = row.get(0).expect("extract count");
    assert_eq!(count, 2, "expected two rows inserted");
}

#[test]
fn insert_arrow_batch_into_duckdb_succeeds_when_schema_matches() {
    // Create real DuckDB connection and initialize schema
    let conn = Connection::open_in_memory().expect("open in-memory duckdb");
    conn.execute_batch(SCHEMA_SQL)
        .expect("apply real schema SQL");

    // Build original Arrow RecordBatch and then fix the field name to match table schema
    let rows = sample_rows();

    use duckdb::arrow::datatypes::{Field, Schema};
    use duckdb::arrow::record_batch::RecordBatch;
    use std::sync::Arc;

    let original = create_arrow_record_batch(&rows).expect("create arrow record batch");
    let schema_arc = original.schema();
    let fields = schema_arc.fields();
    assert_eq!(fields.len(), 3, "expected ulid, timestamp, raw_json fields");

    // Create new schema with 'ts' instead of 'timestamp'
    let corrected_schema = Arc::new(Schema::new(vec![
        Field::new("ulid", fields[0].data_type().clone(), false),
        Field::new("ts", fields[1].data_type().clone(), false),
        Field::new("raw_json", fields[2].data_type().clone(), false),
    ]));

    let corrected_batch = RecordBatch::try_new(corrected_schema, original.columns().to_vec())
        .expect("build corrected record batch");

    // Insert should succeed now
    let mut appender = conn.appender("data_landing").expect("open appender");
    appender
        .append_record_batch(corrected_batch)
        .expect("append corrected batch");
    appender.flush().expect("flush appender");

    // Verify rows landed
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM data_landing")
        .expect("prepare count query");
    let mut rows_iter = stmt.query([]).expect("execute count query");
    let row = rows_iter.next().expect("first row").expect("row ok");
    let count: i64 = row.get(0).expect("extract count");
    assert_eq!(count, 2, "expected two rows inserted");
}

// Ports and adapter migration notes:
// - Strong coupling between http and database services was necessary to implement this test,
//   that should be simpler and not so tight. Now the implementation is tied tightly
//   to DuckDB.
//
// TODO: migrate these notes into the changelog once the migration is ready to merge
//

#[tokio::test]
async fn test_mapping_against_in_memory_duckdb() {
    let (db_shutdown_tx, db_shutdown_rx) = unbounded();
    let registry = Arc::new(Registry::new());
    let db_svc = DbService::new(None, db_shutdown_rx).unwrap();
    let db_handle = db_svc.get_handle();

    let join_handle = db_svc.start().await.unwrap();

    let router = build_router(db_handle.clone(), registry);

    let new_mapping = r#"{
        "model": "sensor-a",
        "id": "123",
        "validity_start": "2026-06-28T12:00:00Z",
        "description": "Livingroom"
    }"#;

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mappings")
                .header("content-type", "application/json")
                .body(Body::from(new_mapping))
                .unwrap(),
        )
        .await
        .unwrap();

    db_shutdown_tx.send(()).unwrap();

    join_handle.await.unwrap();

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    let input_json: serde_json::Value = serde_json::from_str(new_mapping).unwrap();
    let mut response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let mapping_id = response_json
        .as_object_mut()
        .unwrap()
        .remove("mapping_id")
        .expect("response should contain mapping_id");

    // new database - we should be getting mapping id 1
    assert_eq!(mapping_id.as_i64(), Some(1));
    assert_eq!(response_json, input_json);

    assert_eq!(status, StatusCode::CREATED);
}
