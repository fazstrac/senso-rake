use crate::domain::{
    LogicalSensorId, MeasurementKind, PhysicalDeviceId, SeriesBindingId, ValidityInterval,
};

#[allow(dead_code)]
pub struct SeriesBinding {
    id: SeriesBindingId,
    physical_device_id: PhysicalDeviceId,
    logical_sensor_id: LogicalSensorId,
    measurement_kind: MeasurementKind,
    validity: ValidityInterval,
}

#[derive(Debug)]
pub enum SeriesBindingError {}

impl SeriesBinding {
    pub fn new(
        id: SeriesBindingId,
        physical_device_id: PhysicalDeviceId,
        logical_sensor_id: LogicalSensorId,
        measurement_kind: MeasurementKind,
        validity: ValidityInterval,
    ) -> Result<Self, SeriesBindingError> {
        Ok(SeriesBinding {
            id,
            physical_device_id,
            logical_sensor_id,
            measurement_kind,
            validity,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeDelta, Utc};

    #[test]
    fn smoke_test() {
        let ts1 = NaiveDate::from_ymd_opt(2026, 7, 12)
            .unwrap()
            .and_hms_opt(10, 00, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap();
        let ts2 = ts1 + TimeDelta::hours(1);

        let validity_interval = ValidityInterval::new(ts1, Some(ts2)).unwrap();

        SeriesBinding::new(1, 1, 1, MeasurementKind::Humidity, validity_interval).unwrap();
    }
}
