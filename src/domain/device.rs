use crate::domain::{LogicalSensorId, PhysicalDeviceId};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicalDeviceIdentity {
    pub model: String,
    pub reported_id: String,
    pub channel: Option<String>,
}

pub struct PhysicalDevice {
    pub id: PhysicalDeviceId,
    pub identity: PhysicalDeviceIdentity,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub battery_ok: Option<bool>,
    pub rssi: Option<f64>,
}

pub struct LogicalSensor {
    pub id: LogicalSensorId,
    pub display_name: String,
}

#[cfg(test)]
mod tests {}
