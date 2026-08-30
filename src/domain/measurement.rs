use crate::domain::PhysicalDeviceId;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementKind {
    Temperature,
    Humidity,
    Pressure,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Observation {
    physical_device_id: PhysicalDeviceId,
    measurement_kind: MeasurementKind,
    value: f64,
    observed_at: DateTime<Utc>,
    received_at: DateTime<Utc>,
}

#[derive(Debug)]
pub enum ObservationError {
    ReceivedBeforeObservedError,
}

impl Observation {
    pub fn new(
        physical_device_id: PhysicalDeviceId,
        measurement_kind: MeasurementKind,
        value: f64,
        observed_at: DateTime<Utc>,
        received_at: DateTime<Utc>,
    ) -> Result<Observation, ObservationError> {
        // Be careful! This assumption may be very wrong
        // Compare with actual data
        match received_at >= observed_at {
            true => Ok(Observation {
                physical_device_id,
                measurement_kind,
                value,
                observed_at,
                received_at,
            }),
            false => Err(ObservationError::ReceivedBeforeObservedError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeDelta, Utc};

    #[test]
    fn smoke_test() {
        let ts_obs = NaiveDate::from_ymd_opt(2026, 7, 12)
            .unwrap()
            .and_hms_opt(10, 00, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap();

        let ts_recv = ts_obs + TimeDelta::microseconds(23);

        Observation::new(1, MeasurementKind::Humidity, 21.5, ts_obs, ts_recv).unwrap();
    }
}
