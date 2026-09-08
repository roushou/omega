//! State topic addresses.
//!
//! `StateTopic.topic` is a string on the wire, and this is the edge that
//! validates it: a closed set of system topics, plus one open escape hatch —
//! `unit.<name>.<key>`, the keyspace a unit owns. A typo like `"batery"` is
//! rejected here rather than silently subscribing to nothing.
//!
//! The closed set is declared once, in [`topics!`]. A topic's name, its
//! place in `SystemTopic::ALL`, and the type it carries came from four lists
//! that had to agree; they are now one row, because a topic missing from one
//! of those lists is a topic nothing can be checked against.

use std::fmt;

use crate::omega::{
    AudioState, BacklightState, BatteryState, BluetoothState, DisplayState, IdleState, MediaState,
    NetworkState, PowerState, SystemState, TimeState, UnitsState, WifiState, WindowState,
    WorkspacesState, state_topic,
};

/// Declare the system topics: the enum, `ALL`, the wire name, and the value
/// type each one carries.
///
/// The variant is also the `state_topic::Value` variant, which prost names
/// after the oneof field — so `Battery` here is `battery` there, and a row
/// whose proto field is not the snake_case of its variant will not compile.
macro_rules! topics {
    ($(
        $(#[$meta:meta])*
        $variant:ident => $name:literal : $value:ty,
    )*) => {
        /// The system topics the daemon publishes. Closed: the ontology grows
        /// in `state.proto` and in the table below, never at a call site.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum SystemTopic {
            $($(#[$meta])* $variant,)*
        }

        impl SystemTopic {
            /// Every topic, in declaration order. Generated from the same row
            /// as the variant, so it cannot omit one.
            pub const ALL: &'static [SystemTopic] = &[$(Self::$variant,)*];

            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }
        }

        $(
            impl TopicValue for $value {
                const TOPIC: SystemTopic = SystemTopic::$variant;

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
        )*
    };
}

topics! {
    Battery => "battery": BatteryState,
    Network => "network": NetworkState,
    Audio => "audio": AudioState,
    Backlight => "backlight": BacklightState,
    Power => "power": PowerState,
    Display => "display": DisplayState,
    /// The supervisor's report on every unit it runs.
    Units => "units": UnitsState,
    Time => "time": TimeState,
    /// What the last scan found on the air.
    Wifi => "wifi": WifiState,
    Workspaces => "workspaces": WorkspacesState,
    /// What has focus.
    Window => "window": WindowState,
    /// What is playing.
    Media => "media": MediaState,
    Bluetooth => "bluetooth": BluetoothState,
    /// What the machine is doing with itself.
    System => "system": SystemState,
    /// Whether anybody is using it.
    Idle => "idle": IdleState,
}

impl SystemTopic {
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
