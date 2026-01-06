use serde::{Deserialize, Serialize};

// `Mapping` is the JSON structure accepted by the `/mapping` endpoint.
// Keep it simple: a sensor id, manufacturer and a human-readable name.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mapping {
    pub sensor_id: String,
    pub manufacturer: String,
    pub name: String,
}

/// Compose a key for the internal HashMap. The format is `manufacturer::id`.
/// This keeps keys unique across different manufacturers and is reversible
/// if you need to split them later.
pub fn key_for(sensor_id: &str, manufacturer: &str) -> String {
    format!("{}:{}", manufacturer, sensor_id)
}
