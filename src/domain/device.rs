use crate::domain::{LogicalSensorId, PhysicalDeviceId};
use chrono::{DateTime, Utc};

#[derive(Debug)]
pub enum LogicalSensorError {}

#[allow(dead_code)]
pub struct LogicalSensor {
    id: LogicalSensorId,
    display_name: String,
}

impl LogicalSensor {
    pub fn new(id: LogicalSensorId, display_name: String) -> Result<Self, LogicalSensorError> {
        Ok(LogicalSensor { id, display_name })
    }
}

#[derive(Debug, Clone)]
pub struct PhysicalDeviceIdentity {
    model: String,
    reported_id: String,
    channel: Option<String>,
}

// Define PartialEq explicitly to be able define the equality
// or redefine it later if necessary - eg. dropping case-sensitivity
impl PartialEq for PhysicalDeviceIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.model == other.model
            && self.reported_id == other.reported_id
            && self.channel == other.channel
    }
}

#[derive(Debug)]
pub enum PhysicalDeviceIdentityError {}

impl PhysicalDeviceIdentity {
    pub fn new(
        model: String,
        reported_id: String,
        channel: Option<String>,
    ) -> Result<Self, PhysicalDeviceIdentityError> {
        Ok(PhysicalDeviceIdentity {
            model,
            reported_id,
            channel,
        })
    }
}

#[allow(dead_code)]
pub struct PhysicalDevice {
    id: PhysicalDeviceId,
    identity: PhysicalDeviceIdentity,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    battery_ok: Option<bool>,
    rssi: Option<f64>,
}

#[derive(Debug)]
pub enum PhysicalDeviceError {
    LastSeenBeforeFirstSeenError,
}

impl PhysicalDevice {
    pub fn new(
        id: PhysicalDeviceId,
        identity: PhysicalDeviceIdentity,
        first_seen: DateTime<Utc>,
        last_seen: DateTime<Utc>,
        battery_ok: Option<bool>,
        rssi: Option<f64>,
    ) -> Result<PhysicalDevice, PhysicalDeviceError> {
        match last_seen >= first_seen {
            true => Ok(PhysicalDevice {
                id,
                identity,
                first_seen,
                last_seen,
                battery_ok,
                rssi,
            }),
            false => Err(PhysicalDeviceError::LastSeenBeforeFirstSeenError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeDelta, Utc};

    #[test]
    fn smoke_test() {
        let _ls = LogicalSensor::new(1, "Test".into()).unwrap();
        let fs = NaiveDate::from_ymd_opt(2026, 7, 12)
            .unwrap()
            .and_hms_opt(10, 00, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap();

        let ls = fs + TimeDelta::microseconds(23);

        let pdi = PhysicalDeviceIdentity::new("Test".into(), "245".into(), Some("Channel".into()))
            .unwrap();
        PhysicalDevice::new(1, pdi, fs, ls, Some(true), Some(5.0)).unwrap();
    }

    #[test]
    fn physicaldeviceidentity_should_be_equal_with_itself() {
        let identity1 =
            PhysicalDeviceIdentity::new("Test".into(), "245".into(), Some("Channel".into()))
                .unwrap();

        let identity2 = PhysicalDeviceIdentity::new("Test".into(), "245".into(), None).unwrap();

        assert_eq!(
            identity1, identity1,
            "Identity should be equal with itself when channel is Some"
        );
        assert_eq!(
            identity2, identity2,
            "Identity should be equal with itself when channel is None"
        );
    }

    struct PhysicalDeviceIdentityTestCase {
        name: &'static str,
        this: PhysicalDeviceIdentity,
        other: PhysicalDeviceIdentity,
        should_be_equal: bool,
    }

    fn identity(model: &str, reported_id: &str, channel: Option<String>) -> PhysicalDeviceIdentity {
        PhysicalDeviceIdentity::new(model.into(), reported_id.into(), channel).unwrap()
    }

    #[test]
    fn exercise_physicaldeviceidentity_equality() {
        let testcases = vec![
            PhysicalDeviceIdentityTestCase {
                name: "identities should be equal when when both channels are None",
                this: identity("Test", "245", None),
                other: identity("Test", "245", None),
                should_be_equal: true,
            },
            PhysicalDeviceIdentityTestCase {
                name: "identities should be equal when when both channels are same Some",
                this: identity("Test", "245", Some("Channel".into())),
                other: identity("Test", "245", Some("Channel".into())),
                should_be_equal: true,
            },
            PhysicalDeviceIdentityTestCase {
                name: "identities should not be equal when this channel is Some and other channel is None",
                this: identity("Test", "245", Some("Channel".into())),
                other: identity("Test", "245", None),
                should_be_equal: false,
            },
            PhysicalDeviceIdentityTestCase {
                name: "identities should not be equal when models are different",
                this: identity("Test1", "245", Some("Channel".into())),
                other: identity("Test2", "245", Some("Channel".into())),
                should_be_equal: false,
            },
            PhysicalDeviceIdentityTestCase {
                name: "identities should not be equal when reported_ids are different",
                this: identity("Test", "245", Some("Channel".into())),
                other: identity("Test", "246", Some("Channel".into())),
                should_be_equal: false,
            },
            PhysicalDeviceIdentityTestCase {
                name: "identities should not be equal when channels are different",
                this: identity("Test", "245", Some("Channel1".into())),
                other: identity("Test", "245", Some("Channel2".into())),
                should_be_equal: false,
            },
        ];

        for case in testcases {
            let name = case.name;
            assert_eq!(case.this == case.other, case.should_be_equal, "{name}");
        }
    }
}
