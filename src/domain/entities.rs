use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};


#[derive(Debug, Deserialize, Serialize)]
pub struct SensorMapping {
    pub mapping_id: Option<i64>,
    pub model: String,
    pub id: String,
    pub description: Option<String>,
    pub validity_start: Option<DateTime<Utc>>,
    pub deleted: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawMessage {
    pub ulid: String,
    pub timestamp_us: i64,
    pub raw_json: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NewMapping {
    pub model: String,
    pub id: String,
    pub description: String,
    pub validity_start: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MappingId(pub i64);