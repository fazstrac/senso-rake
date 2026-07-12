use chrono::{DateTime, Utc};

#[derive(Debug, PartialEq)]
pub struct ValidityInterval {
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
}

#[derive(Debug, PartialEq)]
pub enum ValidityError {
    EmptyInterval,
}

impl ValidityInterval {
    pub fn new(
        valid_from: DateTime<Utc>,
        valid_until: Option<DateTime<Utc>>,
    ) -> Result<Self, ValidityError> {
        match valid_until {
            Some(this_valid_until) => match valid_from < this_valid_until {
                true => Ok(ValidityInterval {
                    valid_from,
                    valid_until: Some(this_valid_until),
                }),
                false => Err(ValidityError::EmptyInterval),
            },
            None => Ok(ValidityInterval {
                valid_from,
                valid_until: None,
            }),
        }
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        match (self.valid_until, other.valid_until) {
            (Some(valid_until), Some(other_valid_until)) => {
                self.valid_from < other_valid_until && valid_until > other.valid_from
            }
            (None, Some(other_valid_until)) => other_valid_until > self.valid_from,
            (Some(valid_until), None) => valid_until > other.valid_from,
            (None, None) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::{NaiveDate, TimeDelta, Utc};

    #[test]
    fn constructing_validity_interval_with_some_valid_until_should_succeed() {
        let ts1 = NaiveDate::from_ymd_opt(2026, 7, 12)
            .unwrap()
            .and_hms_opt(10, 00, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap();
        let ts2 = ts1 + chrono::TimeDelta::hours(1);

        // Deliberately testing the ::new against building ValidityInterval directly
        assert_eq!(
            ValidityInterval::new(ts1, Some(ts2)),
            Ok(ValidityInterval {
                valid_from: ts1,
                valid_until: Some(ts2)
            }),
            "ValidityInterval should succeed"
        )
    }

    #[test]
    fn constructing_validity_interval_with_none_valid_until_should_succeed() {
        let ts1 = NaiveDate::from_ymd_opt(2026, 7, 12)
            .unwrap()
            .and_hms_opt(10, 00, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap();

        // Deliberately testing the ::new against building ValidityInterval directly
        assert_eq!(
            ValidityInterval::new(ts1, None),
            Ok(ValidityInterval {
                valid_from: ts1,
                valid_until: None
            }),
            "ValidityInterval should succeed"
        )
    }

    #[test]
    fn constructing_invalid_validity_interval_should_fail() {
        let ts1 = NaiveDate::from_ymd_opt(2026, 7, 12)
            .unwrap()
            .and_hms_opt(10, 00, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap();

        let ts2 = ts1 + chrono::TimeDelta::hours(-1);

        assert_eq!(
            ValidityInterval::new(ts1, Some(ts2)),
            Err(ValidityError::EmptyInterval),
            "Interval should be empty"
        )
    }

    struct ValidityIntervalOverlapTestCase {
        name: &'static str,
        first: ValidityInterval,
        second: ValidityInterval,
        should_overlap: bool,
    }

    fn validity_interval(offset_start: i64, offset_end_to_start: Option<i64>) -> ValidityInterval {
        let ts_base: DateTime<Utc> = NaiveDate::from_ymd_opt(2026, 7, 12)
            .unwrap()
            .and_hms_opt(10, 00, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap();

        let ts_begin = ts_base + TimeDelta::minutes(offset_start);
        let ts_end =
            offset_end_to_start.map(|offset| ts_base + TimeDelta::minutes(offset_start + offset));

        ValidityInterval::new(ts_begin, ts_end).unwrap()
    }

    #[test]
    fn validity_interval_overlap_testcases() {
        let testcases = vec![
            ValidityIntervalOverlapTestCase {
                name: "intervals where second starts after first starts and before first ends should overlap",
                first: validity_interval(0, Some(60)),
                second: validity_interval(30, Some(60)),
                should_overlap: true,
            },
            ValidityIntervalOverlapTestCase {
                name: "intervals with one open-ended should overlap case 1",
                first: validity_interval(0, None),
                second: validity_interval(30, Some(60)),
                should_overlap: true,
            },
            ValidityIntervalOverlapTestCase {
                name: "intervals with one open-ended should overlap case 2",
                first: validity_interval(0, Some(60)),
                second: validity_interval(30, None),
                should_overlap: true,
            },
            ValidityIntervalOverlapTestCase {
                name: "intervals where second interval starts where first ends should not overlap",
                first: validity_interval(0, Some(60)),
                second: validity_interval(60, Some(60)),
                should_overlap: false,
            },
            ValidityIntervalOverlapTestCase {
                name: "non-overlapping intervals should not overlap",
                first: validity_interval(0, Some(60)),
                second: validity_interval(-60, Some(30)),
                should_overlap: false,
            },
            ValidityIntervalOverlapTestCase {
                name: "intervals where first is open-ended and second ends before first starts should not overlap",
                first: validity_interval(0, None),
                second: validity_interval(-60, Some(60)),
                should_overlap: false,
            },
            ValidityIntervalOverlapTestCase {
                name: "interval where both are open-ended intervals should overlap",
                first: validity_interval(0, None),
                second: validity_interval(60, None),
                should_overlap: true,
            },
        ];

        for case in testcases {
            let name = case.name;

            assert_eq!(
                case.second.overlaps(&case.first),
                case.should_overlap,
                "{name}"
            );
            assert_eq!(
                case.first.overlaps(&case.second),
                case.should_overlap,
                "{name}"
            );
        }
    }
}
