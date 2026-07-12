use crate::domain::{LogicalSensorId, PhysicalDeviceId};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct PhysicalDeviceIdentity {
    pub model: String,
    pub reported_id: String,
    pub channel: Option<String>,
}

pub struct PhysicalDevice {
    pub id: PhysicalDeviceId,
    pub identity: PhysicalDeviceIdentity,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub battery_ok: Option<bool>,
    pub rssi: Option<f64>,
}

pub struct LogicalSensor {
    pub id: LogicalSensorId,
    pub display_name: String,
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

#[cfg(test)]
mod tests {
    use crate::domain::PhysicalDeviceIdentity;

    #[test]
    fn physicaldeviceidentity_should_be_equal_with_itself() {
        let identity1 = PhysicalDeviceIdentity {
            model: "Test".into(),
            reported_id: "245".into(),
            channel: Some("Channel".into()),
        };

        let identity2 = PhysicalDeviceIdentity {
            model: "Test".into(),
            reported_id: "245".into(),
            channel: None,
        };

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
        PhysicalDeviceIdentity {
            model: model.into(),
            reported_id: reported_id.into(),
            channel,
        }
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
