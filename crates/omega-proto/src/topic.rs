//! State topic addresses.
//!
//! `StateTopic.topic` is a string on the wire, and this is the edge that
//! validates it: a closed set of system topics, plus one open escape hatch —
//! `unit.<name>.<key>`, the keyspace a unit owns. A typo like `"batery"` is
//! rejected here rather than silently subscribing to nothing.

use std::fmt;

use crate::omega::{
    AudioState, BacklightState, BatteryState, DisplayState, NetworkState, PowerState, UnitsState,
    state_topic,
};

/// The system topics the daemon publishes. Closed: the ontology grows in
/// `state.proto`, not at a call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SystemTopic {
    Battery,
    Network,
    Audio,
    Backlight,
    Power,
    Display,
    /// The supervisor's report on every unit it runs.
    Units,
}

impl SystemTopic {
    pub const ALL: &'static [SystemTopic] = &[
        Self::Battery,
        Self::Network,
        Self::Audio,
        Self::Backlight,
        Self::Power,
        Self::Display,
        Self::Units,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Battery => "battery",
            Self::Network => "network",
            Self::Audio => "audio",
            Self::Backlight => "backlight",
            Self::Power => "power",
            Self::Display => "display",
            Self::Units => "units",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.as_str() == name)
    }
}

impl fmt::Display for SystemTopic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A validated topic address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Topic {
    /// One of the daemon's own topics.
    System(SystemTopic),
    /// `unit.<name>.<key>` — data a unit owns and only it may write.
    Unit { unit: String, key: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TopicError {
    #[error("unknown state topic {0:?}")]
    Unknown(String),
    #[error("unit topic {0:?} must be shaped unit.<name>.<key>")]
    MalformedUnitTopic(String),
}

impl Topic {
    pub const UNIT_PREFIX: &'static str = "unit.";

    pub fn parse(address: &str) -> Result<Self, TopicError> {
        if let Some(rest) = address.strip_prefix(Self::UNIT_PREFIX) {
            let (unit, key) = rest
                .split_once('.')
                .ok_or_else(|| TopicError::MalformedUnitTopic(address.to_string()))?;
            if unit.is_empty() || key.is_empty() {
                return Err(TopicError::MalformedUnitTopic(address.to_string()));
            }
            return Ok(Self::Unit {
                unit: unit.to_string(),
                key: key.to_string(),
            });
        }

        SystemTopic::parse(address)
            .map(Self::System)
            .ok_or_else(|| TopicError::Unknown(address.to_string()))
    }

    /// The address of a unit's own key.
    pub fn of_unit(unit: &str, key: &str) -> Self {
        Self::Unit {
            unit: unit.to_string(),
            key: key.to_string(),
        }
    }

    /// The unit that may write this topic. System topics are the daemon's:
    /// no unit writes them, whatever capability it holds.
    pub fn owner(&self) -> Option<&str> {
        match self {
            Self::System(_) => None,
            Self::Unit { unit, .. } => Some(unit),
        }
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::System(topic) => f.write_str(topic.as_str()),
            Self::Unit { unit, key } => write!(f, "{}{unit}.{key}", Self::UNIT_PREFIX),
        }
    }
}

/// A topic's value type.
///
/// The ontology in `state.proto` is closed, so the mapping from a topic to
/// the type it carries can be too: a reader names the type it wants and gets
/// it or nothing, instead of matching a oneof at every call site.
pub trait TopicValue: Sized {
    /// The topic this type is the value of.
    const TOPIC: SystemTopic;

    /// Extract this type from a topic's value, if that is what it holds.
    fn of(value: &state_topic::Value) -> Option<&Self>;

    /// Put this type back into a topic's value. The inverse of [`of`], so a
    /// test can build the state a unit reads with the same types the unit
    /// reads it with.
    ///
    /// [`of`]: Self::of
    fn into_value(self) -> state_topic::Value;
}

macro_rules! topic_value {
    ($type:ty, $topic:ident, $variant:ident) => {
        impl TopicValue for $type {
            const TOPIC: SystemTopic = SystemTopic::$topic;

            fn of(value: &state_topic::Value) -> Option<&Self> {
                match value {
                    state_topic::Value::$variant(value) => Some(value),
                    _ => None,
                }
            }

            fn into_value(self) -> state_topic::Value {
                state_topic::Value::$variant(self)
            }
        }
    };
}

topic_value!(BatteryState, Battery, Battery);
topic_value!(NetworkState, Network, Network);
topic_value!(AudioState, Audio, Audio);
topic_value!(BacklightState, Backlight, Backlight);
topic_value!(PowerState, Power, Power);
topic_value!(DisplayState, Display, Display);
topic_value!(UnitsState, Units, Units);
