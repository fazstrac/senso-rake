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

#[derive(Debug, PartialEq)]
pub enum SeriesBindingConflict {
    None,
    LogicalSeries,
    PhysicalSeries,
}

impl SeriesBinding {
    pub fn new(
        id: SeriesBindingId,
        physical_device_id: PhysicalDeviceId,
        logical_sensor_id: LogicalSensorId,
        measurement_kind: MeasurementKind,
        validity: ValidityInterval,
    ) -> Self {
        SeriesBinding {
            id,
            physical_device_id,
            logical_sensor_id,
            measurement_kind,
            validity,
        }
    }

    /*
    logical conflict:
        two physical temperature series both feed Livingroom.temperature at the same time
    physical conflict:
        the same physical temperature series feeds Livingroom.temperature and Bedroom.temperature at the same time
     */
    pub fn conflicts_with(&self, other: &SeriesBinding) -> SeriesBindingConflict {
        // Is the measurement kind the same, eg Humidity
        match self.measurement_kind == other.measurement_kind {
            true => {
                match (
                    self.physical_device_id == other.physical_device_id,
                    self.logical_sensor_id == other.logical_sensor_id,
                ) {
                    (true, false) => match self.validity.overlaps(&other.validity) {
                        true => SeriesBindingConflict::PhysicalSeries,
                        false => SeriesBindingConflict::None,
                    },
                    (false, true) => match self.validity.overlaps(&other.validity) {
                        true => SeriesBindingConflict::LogicalSeries,
                        false => SeriesBindingConflict::None,
                    },
                    (true, true) | (false, false) => SeriesBindingConflict::None,
                }
            }
            false => SeriesBindingConflict::None,
        }
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

        SeriesBinding::new(1, 1, 1, MeasurementKind::Humidity, validity_interval);
    }

    // TODO implement more cases for .conflicts_with
    #[test]
    fn conflict_test_physical_device_changes_non_overlapping_intervals() {
        let ts1_start = NaiveDate::from_ymd_opt(2026, 7, 12)
            .unwrap()
            .and_hms_opt(10, 00, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap();
        let ts1_end = ts1_start + TimeDelta::hours(1);

        let vi1 = ValidityInterval::new(ts1_start, Some(ts1_end)).unwrap();
        let vi2 = ValidityInterval::new(ts1_end, None).unwrap();

        let pdid1: PhysicalDeviceId = 1;
        let pdid2: PhysicalDeviceId = 2;
        let lsid: LogicalSensorId = 1;
        let kind: MeasurementKind = MeasurementKind::Temperature;

        let sb1 = SeriesBinding::new(1, pdid1, lsid, kind, vi1);
        let sb2 = SeriesBinding::new(2, pdid2, lsid, kind, vi2);

        let expected_conflict = SeriesBindingConflict::None;

        assert_eq!(sb1.conflicts_with(&sb2), expected_conflict);
    }
}
