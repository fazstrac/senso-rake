use crate::domain::{
    LogicalSensorId, MeasurementKind, PhysicalDeviceId, SeriesBindingId, ValidityInterval,
};

pub struct SeriesBinding {
    pub id: SeriesBindingId,
    pub physical_device_id: PhysicalDeviceId,
    pub logical_sensor_id: LogicalSensorId,
    pub measurement_kind: MeasurementKind,
    pub validity: ValidityInterval,
}
