use crate::mqtt::msg_hash::generate_dedup_ulid;

use anyhow::Result;
use chrono::{NaiveDateTime, TimeZone, Local, Utc};
use duckdb::arrow::array::{TimestampMicrosecondArray, StringArray};
use duckdb::arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use duckdb::arrow::record_batch::RecordBatch;
use std::sync::Arc;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RawMessage {
    time: String,
}

#[derive(Debug, Clone)]
pub struct ProcessedMsg {
    // Universally Unique Lexicographically Sortable Identifier
    ulid: Option<String>,
    // microseconds since epoch; suitable for Arrow Timestamp(Microsecond)
    timestamp: i64,
    raw_json: Option<String>,
}


// Normalize one JSON message string into ProcessedMsg entry
// On error, returns a single row with error info and raw JSON preserved
pub fn process_message(json_str: &str) -> ProcessedMsg {
    let raw: RawMessage = match serde_json::from_str(json_str) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("Error parsing JSON message: {}", err);
            // Save current timestamp for error case
            // Same timestamp should be used for ULID generation
            let ts = Utc::now().timestamp_micros();
            // Return a special row with raw JSON preserved
            return ProcessedMsg {
                ulid: Some(generate_dedup_ulid(ts as u64 / 1000, json_str.as_bytes()).to_string()),
                timestamp: ts, // or a sentinel
                raw_json: Some(json_str.to_string()), // keep the original string
            };
        }
    };

    let ts = parse_time(&raw.time);

    ProcessedMsg {
        ulid: Some(generate_dedup_ulid(ts as u64 / 1000, json_str.as_bytes()).to_string()),
        timestamp: ts,
        raw_json: Some(json_str.to_string()),
    }
}

fn parse_time(val_opt: &String) -> i64 {
    let s = val_opt.as_str();

    // Try to parse the time string as Unix epoch seconds with microseconds (f64)
    // If parsing fails, try to parse in "YYYY-MM-DD HH:MM:SS" format
    
    if !s.is_empty() {
        // First, attempt to parse as f64 (Unix epoch seconds with fractional microseconds)
        if let Ok(epoch_f64) = s.parse::<f64>() {
            return (epoch_f64 * 1_000_000.0) as i64;
        }

        // Then, try to parse in "YYYY-MM-DD HH:MM:SS" format
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
            if let Some(local_dt) = Local.from_local_datetime(&ndt).single() {
                return local_dt.with_timezone(&Utc).timestamp_micros();
            }
        }
    }

    Utc::now().timestamp_micros()
}

pub fn create_arrow_record_batch(rows: &Vec<ProcessedMsg>) -> Result<RecordBatch> {
    let ulid_arr = StringArray::from(
        rows.iter()
            .map(|r| r.ulid.clone().unwrap_or_default())
            .collect::<Vec<String>>(),
    );
    let ra = TimestampMicrosecondArray::from(
        rows.iter().map(|r| r.timestamp).collect::<Vec<i64>>()
    );
    let raw_arr = StringArray::from(
        rows.iter()
            .map(|r| r.raw_json.clone().unwrap_or_default())
            .collect::<Vec<String>>(),
    );

    let schema = Arc::new(Schema::new(vec![
        Field::new("ulid", DataType::Utf8, false),
        Field::new("timestamp", DataType::Timestamp(TimeUnit::Microsecond, None), false),
        Field::new("raw_json", DataType::Utf8, false),
    ]));

    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(ulid_arr),
            Arc::new(ra),
            Arc::new(raw_arr),
        ],
    )?;

    Ok(batch)
}


//
//   TESTS
//

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_JSON: &str = r#"[
    {
        "time": "2025-11-29 22:00:39",
        "model": "LaCrosse-TX29IT",
        "id": 19,
        "battery_ok": 1,
        "newbattery": 0,
        "temperature_C": 20.9,
        "mic": "CRC"
    },
    {
        "time": "2025-11-29 22:00:59",
        "model": "LaCrosse-TX141Bv3",
        "id": 246,
        "channel": 1,
        "battery_ok": 1,
        "temperature_C": 22.0,
        "test": "No"
    },
    {
        "time": "1767723037.402694",
        "protocol": 73,
        "model": "LaCrosse-TX141Bv3",
        "id": 246,
        "channel": 1,
        "battery_ok": 1,
        "temperature_C": 21.2,
        "test": "No",
        "mod": "ASK",
        "freq": 434.01203,
        "rssi": -0.308113,
        "snr": 16.87509,
        "noise": -17.1832
    },
    {
        "time": "2025-11-29 22:02:33",
        "model": "Ambientweather-F007TH",
        "id": 141,
        "channel": 1,
        "battery_ok": 0,
        "temperature_C": 18.72223,
        "humidity": 40,
        "mic": "CRC"
    }
    ]"#;

    #[test]
    fn test_normalize_message() {
        let v: serde_json::Value = serde_json::from_str(TEST_JSON).expect("parse test json");
        let arr = v.as_array().expect("expected json array");

        let mut all_rows = Vec::new();
        for item in arr {
            let s = serde_json::to_string(item).unwrap();
            let msg = process_message(&s);
            all_rows.push(msg);
        }

        // Expected 1 row per message, 4 messages
        assert_eq!(all_rows.len(), 4, "expected 4 normalized rows");

        // Check that each row has ulid, timestamp, and raw_json
        for r in &all_rows {
            assert!(r.ulid.is_some(), "ulid should be present");
            assert!(r.timestamp > 0, "timestamp should be parsed");
            assert!(r.raw_json.is_some(), "raw_json should be present");
        }
    }

    #[test]
    fn test_normalize_misbehaving_json() {
        // Provide malformed JSON and ensure normalize_one_message returns
        // a single sentinel row preserving the original string.
        let bad = "{ this is not valid json }";
        let msg = process_message(bad);
        assert_eq!(msg.raw_json.as_deref(), Some(bad));
        assert!(msg.ulid.is_some(), "ulid should be generated for error rows");
    }

    #[test]
    fn test_normalize_erroneous_json() {
        // Provide malformed JSON and ensure normalize_one_message returns
        // a single sentinel row preserving the original string.
        let bad = "{ \"message\": \"this is valid json, but missing expected fields\" }";
        let msg = process_message(bad);
        assert_eq!(msg.raw_json.as_deref(), Some(bad));
        assert!(msg.ulid.is_some(), "ulid should be generated for error rows");
    }

    #[test]
    fn test_create_arrow_record_batch() {
        let v: serde_json::Value = serde_json::from_str(TEST_JSON).expect("parse test json");
        let arr = v.as_array().expect("expected json array");

        let mut all_rows = Vec::new();
        for item in arr {
            let s = serde_json::to_string(item).unwrap();
            let msg = process_message(&s);
            all_rows.push(msg);
        }

        let batch = create_arrow_record_batch(&all_rows).expect("create arrow record batch");

        assert_eq!(batch.num_rows(), all_rows.len(), "record batch row count");
        assert_eq!(batch.num_columns(), 3, "record batch column count");
    }

    #[test]
    fn test_parse_time_unix_epoch() {
        // Test parsing Unix epoch as f64
        let s = "1704643200.123456".to_string();
        let ts = parse_time(&s);
        assert_eq!(ts, 1704643200123456);
    }

    #[test]
    fn test_parse_time_datetime_format() {
        // Test parsing datetime string
        let s = "2025-11-29 22:00:39".to_string();
        let ts = parse_time(&s);
        // Should be a valid timestamp > 0
        assert!(ts > 0, "timestamp should be parsed successfully");
        // Optionally, check approximate value, but depends on timezone
    }

    #[test]
    fn test_parse_time_invalid() {
        // Test invalid input falls back to current time
        let s = "invalid".to_string();
        let ts = parse_time(&s);
        // Should be current time, which is > some known past time
        let past_ts = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap().timestamp_micros();
        assert!(ts > past_ts, "fallback should be current time");
    }

    #[test]
    fn test_parse_time_empty() {
        // Test empty string falls back to current time
        let s = "".to_string();
        let ts = parse_time(&s);
        let past_ts = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap().timestamp_micros();
        assert!(ts > past_ts, "fallback should be current time");
    }
}
