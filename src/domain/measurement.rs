use chrono::{DateTime, Utc};

pub enum MeasurementKind {
    Temperature,
    Humidity,
    Pressure,
}

type PhysicalDeviceId = u64;

pub struct Observation {
    pub physical_device_id: PhysicalDeviceId,
    pub measurement_kind: MeasurementKind,
    pub value: f64,
    pub observed_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {}
