//! Validated system-topic and plugin-record addresses.
//! The topic table binds names to payload types and generates enumeration and lookup.

use std::fmt;

use crate::omega::{
    ApplicationsState, AudioState, BacklightState, BatteryState, BluetoothState, DiskState,
    IdleState, InputState, MainsState, MediaState, MonitorsState, NetworkState, PeripheralsState,
    PluginsState, PowerProfileState, SystemState, ThermalsState, ThroughputState, TimeState,
    VpnState, WifiState, WindowState, WorkspacesState, state_topic,
};

/// Generate topic names, enumeration, and payload mapping from the protocol oneof.
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

            /// Whether this payload belongs to the named system topic.
            pub fn accepts(self, value: &state_topic::Value) -> bool {
                matches!((self, value), $((Self::$variant, state_topic::Value::$variant(_)))|*)
            }

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
    Applications => "applications": ApplicationsState,
    Battery => "battery": BatteryState,
    Network => "network": NetworkState,
    Audio => "audio": AudioState,
    Backlight => "backlight": BacklightState,
    /// Whether it is plugged in. Named for the socket: a battery is a
    /// different reading, and on a desktop this is the only one there is.
    Mains => "mains": MainsState,
    /// The monitors the compositor is driving.
    Monitors => "monitors": MonitorsState,
    /// The supervisor's report on every plugin it runs.
    Plugins => "plugins": PluginsState,
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
    /// Session idle and locked state.
    Idle => "idle": IdleState,
    /// The batteries of things plugged into it.
    Peripherals => "peripherals": PeripheralsState,
    /// How it is being typed at.
    Input => "input": InputState,
    /// The tunnels it is running through.
    Vpn => "vpn": VpnState,
    /// Where it keeps things.
    Disk => "disk": DiskState,
    /// How it is trading performance against power.
    PowerProfile => "power-profile": PowerProfileState,
    /// How much is moving over each interface, per second.
    Throughput => "throughput": ThroughputState,
    /// How hot it is, and what the fans are doing.
    Thermals => "thermals": ThermalsState,
}

impl std::str::FromStr for SystemTopic {
    type Err = AddressError;
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|t| t.as_str() == name)
            .ok_or_else(|| AddressError::Unknown(name.to_owned()))
    }
}

impl fmt::Display for SystemTopic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A validated system-topic or plugin-record address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Address {
    /// One of the daemon's own topics.
    System(SystemTopic),
    /// `plugin.<name>.<key>` — data a plugin owns and only it may write.
    Plugin { plugin: String, key: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AddressError {
    #[error("unknown state topic {0:?}")]
    Unknown(String),
    #[error("plugin topic {0:?} must be shaped plugin.<name>.<key>")]
    MalformedPluginTopic(String),
}

impl std::str::FromStr for Address {
    type Err = AddressError;

    fn from_str(address: &str) -> Result<Self, Self::Err> {
        if let Some(rest) = address.strip_prefix(Self::PLUGIN_PREFIX) {
            let (plugin, key) = rest
                .split_once('.')
                .ok_or_else(|| AddressError::MalformedPluginTopic(address.to_string()))?;
            if plugin.is_empty() || key.is_empty() {
                return Err(AddressError::MalformedPluginTopic(address.to_string()));
            }
            return Ok(Self::Plugin {
                plugin: plugin.to_string(),
                key: key.to_string(),
            });
        }

        address.parse::<SystemTopic>().map(Self::System)
    }
}

impl Address {
    pub const PLUGIN_PREFIX: &'static str = "plugin.";

    /// The address of a plugin's own key.
    pub fn of_plugin(plugin: &str, key: &str) -> Self {
        Self::Plugin {
            plugin: plugin.to_string(),
            key: key.to_string(),
        }
    }

    /// The plugin that may write this topic. System topics are the daemon's:
    /// no plugin writes them, whatever capability it holds.
    pub fn owner(&self) -> Option<&str> {
        match self {
            Self::System(_) => None,
            Self::Plugin { plugin, .. } => Some(plugin),
        }
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::System(topic) => f.write_str(topic.as_str()),
            Self::Plugin { plugin, key } => write!(f, "{}{plugin}.{key}", Self::PLUGIN_PREFIX),
        }
    }
}

/// Typed conversion between a system topic and its protocol payload.
pub trait TopicValue: Sized {
    /// The topic this type is the value of.
    const TOPIC: SystemTopic;

    /// Extract this type from a topic's value, if that is what it holds.
    fn of(value: &state_topic::Value) -> Option<&Self>;

    /// Wrap this payload in a protocol topic value.
    fn into_value(self) -> state_topic::Value;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_namespace_has_no_legacy_aliases() {
        assert_eq!(
            "plugins".parse::<SystemTopic>().unwrap(),
            SystemTopic::Plugins
        );
        let address = "plugin.audio.volume".parse::<Address>().unwrap();
        assert_eq!(address.owner(), Some("audio"));
        assert_eq!(address.to_string(), "plugin.audio.volume");
        assert!("units".parse::<SystemTopic>().is_err());
        assert!("unit.audio.volume".parse::<Address>().is_err());
    }
}
