use crate::domain::MeasurementKind;
use chrono::{DateTime, Utc};

pub type SeriesBindingId = u64;
pub type PhysicalDeviceId = u64;
pub type LogicalSensorId = u64;

pub struct ValidityInterval {
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
}

pub struct SeriesBinding {
    pub id: SeriesBindingId,
    pub physical_device_id: PhysicalDeviceId,
    pub logical_sensor_id: LogicalSensorId,
    pub measurement_kind: MeasurementKind,
    pub validity: ValidityInterval,
}

#[cfg(test)]
mod tests {}
