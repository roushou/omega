//! The machine a test describes.

use omega_proto::omega::{
    BatteryState, NetworkState, StatePatch, StateSnapshot, StateTopic, invoke,
};
use omega_proto::{TopicValue, Values};

use crate::context::Context;

/// The state a plugin reads, built for a test.
///
/// Set topics with the same types the daemon publishes, so a test cannot
/// describe a machine the daemon could not.
#[derive(Debug, Default, Clone)]
pub struct State {
    topics: Vec<StateTopic>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a topic: `State::new().with(BatteryState { .. })`.
    pub fn with<T: TopicValue>(mut self, value: T) -> Self {
        let topic = T::TOPIC.as_str().to_string();
        self.topics.retain(|held| held.topic != topic);
        self.topics.push(StateTopic {
            topic,
            revision: self.topics.len() as u64 + 1,
            value: Some(value.into_value()),
        });
        self
    }

    /// Say this topic has nothing to report: the daemon has spoken about
    /// it and there is no reading. A machine with no battery, an adapter
    /// that is unplugged, a broker that is down.
    pub fn absent(mut self, topic: omega_proto::SystemTopic) -> Self {
        let topic = topic.as_str().to_string();
        self.topics.retain(|held| held.topic != topic);
        self.topics.push(StateTopic {
            topic,
            revision: self.topics.len() as u64 + 1,
            value: None,
        });
        self
    }

    /// The battery, spelled the way a test means it.
    pub fn battery(self, charge: f64, charging: bool) -> Self {
        self.with(BatteryState {
            level: charge,
            charging,
            seconds_to_empty: 0,
            seconds_to_full: 0,
        })
    }

    /// Whether the cable is in. Its own topic, so a test can describe a
    /// desktop — no battery, on mains — which is the case a widget holding
    /// both is most likely to get wrong.
    pub fn mains(self, connected: bool) -> Self {
        self.with(omega_proto::omega::MainsState { connected })
    }

    /// A plugin's own state, as the daemon replicates it.
    pub fn keyspace(mut self, address: &str, values: Values) -> Self {
        self.topics.retain(|held| held.topic != address);
        self.topics.push(StateTopic {
            topic: address.to_string(),
            revision: self.topics.len() as u64 + 1,
            value: Some(omega_proto::omega::state_topic::Value::Generic(
                omega_proto::IntoValue::into_value(values),
            )),
        });
        self
    }

    /// A connected network of this name and strength.
    pub fn network(self, ssid: &str, signal_percent: u32) -> Self {
        self.with(NetworkState {
            connected: true,
            ssid: ssid.to_string(),
            interface: "wlan0".to_string(),
            signal_percent,
            r#type: 0,
        })
    }

    pub(super) fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            topics: self.topics.clone(),
        }
    }

    pub(super) fn patch(&self) -> StatePatch {
        StatePatch {
            topics: self.topics.clone(),
        }
    }

    /// A context holding this state, with effects collected rather than sent.
    pub(super) fn context(&self) -> (Context, tokio::sync::mpsc::UnboundedReceiver<invoke::Op>) {
        let (sender, effects) = tokio::sync::mpsc::unbounded_channel();
        (Context::new(&self.snapshot(), sender), effects)
    }
}
