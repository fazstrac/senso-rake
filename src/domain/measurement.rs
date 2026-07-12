use crate::domain::PhysicalDeviceId;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementKind {
    Temperature,
    Humidity,
    Pressure,
}

pub struct Observation {
    pub physical_device_id: PhysicalDeviceId,
    pub measurement_kind: MeasurementKind,
    pub value: f64,
    pub observed_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {}
