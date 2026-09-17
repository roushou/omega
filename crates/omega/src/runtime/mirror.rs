//! Runtime mirror of system topics and plugin records.

use std::collections::HashMap;

use crate::platform::{
    Reading, ReadingError, applications::Application, audio::Player, bluetooth::BluetoothDevice,
};
use omega_proto::omega::{StatePatch, StateSnapshot, StateTopic};
use omega_proto::{SystemTopic, TopicValue};

#[derive(Debug, Default)]
pub(crate) struct Mirror {
    topics: HashMap<String, StateTopic>,
    errors: HashMap<SystemTopic, ReadingError>,
    pub(crate) applications: Reading<Vec<Application>>,
    pub(crate) media: Reading<Vec<Player>>,
    pub(crate) bluetooth: Reading<Vec<BluetoothDevice>>,
}

impl Mirror {
    pub(crate) fn from_snapshot(snapshot: &StateSnapshot) -> Self {
        let mut mirror = Self::default();
        for topic in &snapshot.topics {
            mirror.insert(topic);
        }
        mirror
    }

    pub(crate) fn apply(&mut self, patch: &StatePatch) -> Vec<ReadingError> {
        let mut errors = Vec::new();
        for topic in &patch.topics {
            if self.insert(topic)
                && let Ok(kind) = topic.topic.parse::<SystemTopic>()
                && let Some(error) = self.error(kind)
            {
                errors.push(error.clone());
            }
        }
        errors
    }

    pub(crate) fn errors(&self) -> impl Iterator<Item = &ReadingError> {
        self.errors.values()
    }

    pub(crate) fn get<T: TopicValue>(&self) -> Option<&T> {
        if self.errors.contains_key(&T::TOPIC) {
            return None;
        }
        T::of(self.topics.get(T::TOPIC.as_str())?.value.as_ref()?)
    }

    /// Return the generic value stored at a plugin record address.
    pub(crate) fn generic(&self, address: &str) -> Option<&omega_proto::omega::Value> {
        match self.topics.get(address)?.value.as_ref()? {
            omega_proto::omega::state_topic::Value::Generic(value) => Some(value),
            _ => None,
        }
    }

    /// Whether the daemon has reported this topic, including explicit absence.
    /// A reported absent value satisfies startup readiness.
    pub(crate) fn knows(&self, topic: SystemTopic) -> bool {
        self.topics.contains_key(topic.as_str())
    }

    pub(crate) fn error(&self, topic: SystemTopic) -> Option<&ReadingError> {
        self.errors.get(&topic)
    }

    fn collection<T, E: std::fmt::Display>(
        topic: &StateTopic,
        kind: SystemTopic,
        result: Option<Result<Vec<T>, E>>,
    ) -> Reading<Vec<T>> {
        match result {
            None => Reading::Unavailable,
            Some(Ok(values)) => Reading::Ready(values),
            Some(Err(error)) => Reading::Invalid(ReadingError {
                topic: kind,
                revision: topic.revision,
                message: error.to_string(),
            }),
        }
    }

    fn validate(&mut self, topic: &StateTopic, kind: SystemTopic) {
        use omega_proto::omega::state_topic::Value;
        self.errors.remove(&kind);
        if topic
            .value
            .as_ref()
            .is_some_and(|value| !kind.accepts(value))
        {
            self.errors.insert(
                kind,
                ReadingError {
                    topic: kind,
                    revision: topic.revision,
                    message: "payload does not match its topic".into(),
                },
            );
        }
        match kind {
            SystemTopic::Applications => {
                self.applications = Self::collection(
                    topic,
                    kind,
                    match &topic.value {
                        Some(Value::Applications(value)) => Some(
                            value
                                .applications
                                .iter()
                                .cloned()
                                .map(Application::try_from)
                                .collect(),
                        ),
                        _ => None,
                    },
                );
                if let Reading::Invalid(error) = &self.applications {
                    self.errors.insert(kind, error.clone());
                }
                if let Some(error) = self.errors.get(&kind) {
                    self.applications = Reading::Invalid(error.clone());
                }
            }
            SystemTopic::Media => {
                self.media = Self::collection(
                    topic,
                    kind,
                    match &topic.value {
                        Some(Value::Media(value)) => Some(
                            value
                                .players
                                .iter()
                                .cloned()
                                .map(Player::try_from)
                                .collect(),
                        ),
                        _ => None,
                    },
                );
                if let Reading::Invalid(error) = &self.media {
                    self.errors.insert(kind, error.clone());
                }
                if let Some(error) = self.errors.get(&kind) {
                    self.media = Reading::Invalid(error.clone());
                }
            }
            SystemTopic::Bluetooth => {
                self.bluetooth = Self::collection(
                    topic,
                    kind,
                    match &topic.value {
                        Some(Value::Bluetooth(value)) => Some(
                            value
                                .devices
                                .iter()
                                .cloned()
                                .map(BluetoothDevice::try_from)
                                .collect(),
                        ),
                        _ => None,
                    },
                );
                if let Reading::Invalid(error) = &self.bluetooth {
                    self.errors.insert(kind, error.clone());
                }
                if let Some(error) = self.errors.get(&kind) {
                    self.bluetooth = Reading::Invalid(error.clone());
                }
            }
            _ => {}
        }
    }

    fn insert(&mut self, topic: &StateTopic) -> bool {
        if self
            .topics
            .get(&topic.topic)
            .is_some_and(|current| current.revision >= topic.revision)
        {
            return false;
        }
        if let Ok(kind) = topic.topic.parse::<SystemTopic>() {
            self.validate(topic, kind);
        }
        self.topics.insert(topic.topic.clone(), topic.clone());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_proto::omega::{BatteryState, state_topic};

    #[test]
    fn stale_patches_cannot_undo_a_snapshot_or_a_removal() {
        let topic = StateTopic {
            topic: "battery".into(),
            revision: 10,
            value: Some(state_topic::Value::Battery(BatteryState {
                level: 0.8,
                ..Default::default()
            })),
        };
        let mut mirror = Mirror::from_snapshot(&StateSnapshot {
            topics: vec![topic.clone()],
        });
        mirror.apply(&StatePatch {
            topics: vec![StateTopic {
                revision: 9,
                value: None,
                ..topic.clone()
            }],
        });
        assert_eq!(mirror.get::<BatteryState>().unwrap().level, 0.8);
        mirror.apply(&StatePatch {
            topics: vec![StateTopic {
                revision: 11,
                value: None,
                ..topic.clone()
            }],
        });
        mirror.apply(&StatePatch {
            topics: vec![topic],
        });
        assert!(mirror.get::<BatteryState>().is_none());
        assert!(mirror.knows(SystemTopic::Battery));
    }
    #[test]
    fn invalid_readings_replace_valid_data_and_recover_only_on_newer_revisions() {
        use omega_proto::omega::{
            Application, ApplicationsState, BluetoothDevice, BluetoothState, MediaState, PlayerInfo,
        };
        let cases = [
            (
                SystemTopic::Applications,
                state_topic::Value::Applications(ApplicationsState {
                    applications: vec![Application {
                        id: "firefox.desktop".into(),
                        ..Default::default()
                    }],
                }),
                state_topic::Value::Applications(ApplicationsState {
                    applications: vec![Application {
                        id: "../bad.desktop".into(),
                        ..Default::default()
                    }],
                }),
            ),
            (
                SystemTopic::Media,
                state_topic::Value::Media(MediaState {
                    players: vec![PlayerInfo {
                        id: "vlc".into(),
                        ..Default::default()
                    }],
                }),
                state_topic::Value::Media(MediaState {
                    players: vec![PlayerInfo {
                        id: "bad player".into(),
                        ..Default::default()
                    }],
                }),
            ),
            (
                SystemTopic::Bluetooth,
                state_topic::Value::Bluetooth(BluetoothState {
                    devices: vec![BluetoothDevice {
                        id: "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF".into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                state_topic::Value::Bluetooth(BluetoothState {
                    devices: vec![BluetoothDevice {
                        id: "bad path".into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            ),
        ];
        for (kind, valid, invalid) in cases {
            let topic = StateTopic {
                topic: kind.to_string(),
                revision: 1,
                value: Some(valid),
            };
            let mut mirror = Mirror::from_snapshot(&StateSnapshot {
                topics: vec![topic.clone()],
            });
            assert!(mirror.error(kind).is_none());
            let invalid = StateTopic {
                revision: 2,
                value: Some(invalid),
                ..topic.clone()
            };
            let errors = mirror.apply(&StatePatch {
                topics: vec![invalid.clone()],
            });
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].topic, kind);
            assert_eq!(errors[0].revision, 2);
            assert!(mirror.knows(kind));
            assert!(
                matches!(kind,
                SystemTopic::Applications if mirror.get::<ApplicationsState>().is_none())
                    || matches!(kind, SystemTopic::Media if mirror.get::<MediaState>().is_none())
                    || matches!(kind, SystemTopic::Bluetooth if mirror.get::<BluetoothState>().is_none())
            );
            assert!(
                mirror
                    .apply(&StatePatch {
                        topics: vec![topic.clone(), invalid.clone()]
                    })
                    .is_empty()
            );
            assert!(mirror.error(kind).is_some());
            let from_snapshot = Mirror::from_snapshot(&StateSnapshot {
                topics: vec![invalid],
            });
            assert_eq!(from_snapshot.error(kind), mirror.error(kind));
            mirror.apply(&StatePatch {
                topics: vec![StateTopic {
                    revision: 3,
                    ..topic.clone()
                }],
            });
            assert!(mirror.error(kind).is_none());
            mirror.apply(&StatePatch {
                topics: vec![StateTopic {
                    revision: 4,
                    value: None,
                    ..topic
                }],
            });
            assert!(mirror.error(kind).is_none());
        }
    }

    #[test]
    fn payload_mismatch_is_invalid_and_empty_collections_are_ready() {
        use omega_proto::omega::ApplicationsState;
        let mut mirror = Mirror::default();
        assert!(matches!(mirror.applications, Reading::Pending));
        let mut topic = StateTopic {
            topic: "applications".into(),
            revision: 1,
            value: Some(state_topic::Value::Battery(BatteryState::default())),
        };
        mirror.apply(&StatePatch {
            topics: vec![topic.clone()],
        });
        assert!(matches!(mirror.applications, Reading::Invalid(_)));
        topic.revision += 1;
        topic.value = Some(state_topic::Value::Applications(
            ApplicationsState::default(),
        ));
        mirror.apply(&StatePatch {
            topics: vec![topic.clone()],
        });
        assert_eq!(mirror.applications, Reading::Ready(vec![]));
        topic.revision += 1;
        topic.value = None;
        mirror.apply(&StatePatch {
            topics: vec![topic],
        });
        assert_eq!(mirror.applications, Reading::Unavailable);
    }
}
