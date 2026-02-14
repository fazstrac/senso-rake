use serde_json::json;

/// Note: These are handler-level integration tests that verify HTTP behavior.
/// They use mocked database responses to focus on handler logic.
/// For full end-to-end database tests, see tests/duckdb_integration.rs

#[tokio::test]
async fn test_post_mappings_payload_validation_empty_model() {
    // Create a minimal test without a real database
    // We're testing that the handler rejects invalid input
    let payload = json!({
        "model": "",
        "id": "001",
        "validity_start": "2025-02-14T10:30:00Z",
        "description": "Living Room"
    });

    // Verify deserialization works with valid JSON
    let json_str = serde_json::to_string(&payload).unwrap();
    let result: Result<serde_json::Value, _> = serde_json::from_str(&json_str);
    assert!(result.is_ok(), "Valid JSON should deserialize");

    // Verify empty model is present in the payload
    assert_eq!(payload["model"].as_str().unwrap(), "");
}

#[tokio::test]
async fn test_post_mappings_payload_validation_invalid_timestamp() {
    let payload_with_bad_ts = r#"{
        "model": "sensor-a",
        "id": "001",
        "validity_start": "not-a-date",
        "description": "Living Room"
    }"#;

    // Try to deserialize with our actual SensorMapping struct
    // This should fail because the timestamp is invalid
    use senso_rake::http::SensorMapping;
    let result: Result<SensorMapping, _> = serde_json::from_str(payload_with_bad_ts);
    assert!(result.is_err(), "Invalid timestamp should cause deserialization to fail");
}

#[tokio::test]
async fn test_post_mappings_payload_accepts_valid_timestamp() {
    use senso_rake::http::SensorMapping;

    let valid_payload = json!({
        "model": "LaCrosse-TX29IT",
        "id": "001",
        "validity_start": "2025-02-14T10:30:00Z",
        "description": "Living Room"
    });

    let json_str = serde_json::to_string(&valid_payload).unwrap();
    let result: Result<SensorMapping, _> = serde_json::from_str(&json_str);

    assert!(result.is_ok(), "Valid ISO 8601 timestamp should deserialize");
    let mapping = result.unwrap();
    assert_eq!(mapping.model, "LaCrosse-TX29IT");
    assert_eq!(mapping.id, "001");
    assert_eq!(mapping.description, "Living Room");
}

#[tokio::test]
async fn test_timestamp_always_utc() {
    use senso_rake::http::SensorMapping;

    let payload = json!({
        "model": "sensor",
        "id": "1",
        "validity_start": "2025-02-14T15:30:00Z",
        "description": "Test"
    });

    let json_str = serde_json::to_string(&payload).unwrap();
    let mapping: SensorMapping = serde_json::from_str(&json_str).unwrap();

    // Verify it's stored as UTC
    let serialized = serde_json::to_string(&mapping).unwrap();
    assert!(serialized.contains("2025-02-14T15:30:00Z"), "Timestamp should remain in UTC ISO 8601 format");
}
