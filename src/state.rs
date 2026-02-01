use serde::{Deserialize, Serialize};

// Note:
// This is not current used, it is a placeholder for future functionality.


// `Mapping` is the JSON structure accepted by the `/mapping` endpoint.
// Keep it simple: a sensor id, manufacturer and a human-readable name.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mapping {
    pub sensor_id: String,
    pub manufacturer: String,
    pub name: String,
}

