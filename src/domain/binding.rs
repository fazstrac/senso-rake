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

    Note: overlapping bindings with the same physical device, same logical sensor, and same
    measurement kind currently count as no conflict. If those represent distinct persisted
    binding records rather than the same assignment being compared with itself, this may later
    deserve a dedicated Duplicate/Redundant conflict variant.
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

    struct SeriesBindingConflictTestCase {
        name: &'static str,
        sb1: SeriesBinding,
        sb2: SeriesBinding,
        expected_conflict: SeriesBindingConflict,
    }

    fn build_test_case(
        name: &'static str,
        same_logical_sensor: bool,
        same_measurement_kind: bool,
        same_physical_device: bool,
        overlapping_interval: bool,
        expected_conflict: SeriesBindingConflict,
    ) -> SeriesBindingConflictTestCase {
        let ts1_start = NaiveDate::from_ymd_opt(2026, 7, 12)
            .unwrap()
            .and_hms_opt(10, 00, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap();
        let ts1_end = ts1_start + TimeDelta::hours(1);

        let vi1 = ValidityInterval::new(ts1_start, Some(ts1_end)).unwrap();

        let vi2 = match overlapping_interval {
            true => ValidityInterval::new(ts1_start, None).unwrap(),
            false => ValidityInterval::new(ts1_end, None).unwrap(),
        };

        let pdid1: PhysicalDeviceId = 1;
        let lsid1: LogicalSensorId = 1;
        let kind1: MeasurementKind = MeasurementKind::Temperature;

        let pdid2: PhysicalDeviceId = 2;
        let lsid2: LogicalSensorId = 2;
        let kind2: MeasurementKind = MeasurementKind::Humidity;

        let sb1 = SeriesBinding::new(1, pdid1, lsid1, kind1, vi1);

        let sb2 = match (
            same_physical_device,
            same_logical_sensor,
            same_measurement_kind,
        ) {
            (true, true, true) => SeriesBinding::new(2, pdid1, lsid1, kind1, vi2),
            (false, true, true) => SeriesBinding::new(2, pdid2, lsid1, kind1, vi2),
            (true, false, true) => SeriesBinding::new(2, pdid1, lsid2, kind1, vi2),
            (false, false, true) => SeriesBinding::new(2, pdid2, lsid2, kind1, vi2),
            (true, true, false) => SeriesBinding::new(2, pdid1, lsid1, kind2, vi2),
            (false, true, false) => SeriesBinding::new(2, pdid2, lsid1, kind2, vi2),
            (true, false, false) => SeriesBinding::new(2, pdid1, lsid2, kind2, vi2),
            (false, false, false) => SeriesBinding::new(2, pdid2, lsid2, kind2, vi2),
        };

        SeriesBindingConflictTestCase {
            name,
            sb1,
            sb2,
            expected_conflict,
        }
    }

    #[test]
    fn seriesbindingconflict_test_cases() {
        let test_cases = vec![
            // different measurement kind never conflicts
            build_test_case(
                "case 1: different measurement kind never conflicts",
                true,
                false,
                true,
                true,
                SeriesBindingConflict::None,
            ),
            build_test_case(
                "case 2: different measurement kind never conflicts",
                false,
                false,
                true,
                true,
                SeriesBindingConflict::None,
            ),
            build_test_case(
                "case 3: different measurement kind never conflicts",
                true,
                false,
                false,
                true,
                SeriesBindingConflict::None,
            ),
            build_test_case(
                "case 4: different measurement kind never conflicts",
                false,
                false,
                false,
                true,
                SeriesBindingConflict::None,
            ),
            build_test_case(
                "case 5: different measurement kind never conflicts",
                true,
                false,
                true,
                false,
                SeriesBindingConflict::None,
            ),
            build_test_case(
                "case 6: different measurement kind never conflicts",
                false,
                false,
                true,
                false,
                SeriesBindingConflict::None,
            ),
            build_test_case(
                "case 7: different measurement kind never conflicts",
                true,
                false,
                false,
                false,
                SeriesBindingConflict::None,
            ),
            build_test_case(
                "case 8: different measurement kind never conflicts",
                false,
                false,
                false,
                false,
                SeriesBindingConflict::None,
            ),
            // Baseline test case
            // should not conflict
            build_test_case(
                "case 9: baseline",
                true,
                true,
                false,
                false,
                SeriesBindingConflict::None,
            ),
            // same logical sensor
            // same measurement kind
            // different physical device
            // overlapping interval
            // => LogicalSeries
            build_test_case(
                "case 10",
                true,
                true,
                false,
                true,
                SeriesBindingConflict::LogicalSeries,
            ),
            // same logical sensor
            // same measurement kind
            // different physical device
            // adjacent interval
            // => None
            build_test_case(
                "case 11",
                true,
                true,
                false,
                false,
                SeriesBindingConflict::None,
            ),
            // same physical device
            // same measurement kind
            // different logical sensor
            // overlapping interval
            // => PhysicalSeries
            build_test_case(
                "case 12",
                false,
                true,
                true,
                true,
                SeriesBindingConflict::PhysicalSeries,
            ),
            // same physical device
            // same measurement kind
            // different logical sensor
            // adjacent interval
            // => None
            build_test_case(
                "case 13",
                false,
                true,
                true,
                false,
                SeriesBindingConflict::None,
            ),
            // different physical device
            // different logical sensor
            // same measurement kind
            // overlapping interval
            // => None
            build_test_case(
                "case 14",
                false,
                true,
                false,
                true,
                SeriesBindingConflict::None,
            ),
        ];

        for case in test_cases {
            let name = case.name;
            let sb1 = case.sb1;
            let sb2 = case.sb2;

            assert_eq!(
                sb1.conflicts_with(&sb2),
                case.expected_conflict,
                "{name} sb1->sb2"
            );
            assert_eq!(
                sb2.conflicts_with(&sb1),
                case.expected_conflict,
                "{name} sb2->sb1"
            );
        }
    }
}
