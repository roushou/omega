//! What a plugin can read.
//!
//! One handle per topic the daemon replicates. Holding one is the whole
//! declaration: the field's type names the topic, asks for permission to read
//! it, and hands back values in units that print themselves.
//!
//! Every handle reads through to whatever the daemon last published, so a
//! render is always of the current picture and never of a snapshot taken at
//! construction.
//!
//! A unit's own state lives here too — see [`Own`] and [`Watch`].
//!
//! # One handle per topic, always
//!
//! [`handles!`] declares one for every topic the ontology has, so a topic is
//! reachable the day it is declared. Typed accessors are an upgrade on top —
//! `battery.charge()` gives a [`Percent`] where `get()` gives the wire's
//! `f64` — and a topic without them is still readable rather than published
//! into a void.
//!
//! Twelve brokers were written before this, and thirteen of their topics were
//! replicated into units that had no way to name them. Nothing caught it: the
//! broker coverage test looks the other way down the same pipe.
//!
//! [`Percent`]: crate::units::Percent

mod audio;
mod backlight;
mod battery;
mod bluetooth;
mod clock;
mod disk;
mod idle;
mod input;
mod keyspace;
mod media;
mod monitors;
mod network;
mod peripherals;
mod power;
mod supervised;
mod system;
mod vpn;
mod wifi;
mod window;
mod workspaces;

pub use bluetooth::BluetoothDevice;
pub use clock::Weekday;
pub use disk::Mount;
pub use keyspace::{Own, UnitState, Watch};
pub use media::{Playback, Player};
pub use monitors::Monitor;
pub use peripherals::{Peripheral, PeripheralKind};
pub use supervised::{UnitPhase, UnitReport};
pub use system::{Load, Memory};
pub use vpn::Tunnel;
pub use wifi::AccessPoint;
pub use window::Focused;
pub use workspaces::Workspace;

use crate::context::Context;

/// Declare a handle for every topic: the struct, its wiring, and the list a
/// test compares against the ontology.
///
/// The list is what makes the guarantee structural. It is a second enumeration
/// of the topics and it can drift — but only until the next `cargo test`,
/// which is the same bargain the broker coverage test makes and the same one
/// that has held.
macro_rules! handles {
    ($(
        $(#[$meta:meta])*
        $handle:ident : $value:ty,
    )*) => {
        $(
            $(#[$meta])*
            #[derive(Debug)]
            pub struct $handle {
                context: Context,
            }

            reads!($handle, $value);
        )*

        /// Every topic something can read, compared against
        /// `SystemTopic::ALL` below — so a topic added to the ontology and
        /// not here fails the build rather than becoming unreachable.
        ///
        /// Only compiled for the test that reads it: the guarantee is worth
        /// having, the list is not worth carrying into a unit's binary.
        #[cfg(test)]
        const HANDLED: &[omega_proto::SystemTopic] =
            &[$(<$value as omega_proto::TopicValue>::TOPIC,)*];
    };
}

handles! {
    /// The machine's battery.
    Battery: omega_proto::omega::BatteryState,
    /// Whether it is running on mains.
    Power: omega_proto::omega::PowerState,
    /// The batteries of things plugged into it.
    Peripherals: omega_proto::omega::PeripheralsState,

    /// The connection the machine has.
    Network: omega_proto::omega::NetworkState,
    /// The networks it could have.
    Wifi: omega_proto::omega::WifiState,
    /// The tunnels it is running through.
    Vpn: omega_proto::omega::VpnState,
    /// The adapter and the devices paired with it.
    Bluetooth: omega_proto::omega::BluetoothState,

    /// The output volume.
    Audio: omega_proto::omega::AudioState,
    /// What is playing, and where.
    Media: omega_proto::omega::MediaState,

    /// The screen's brightness.
    Backlight: omega_proto::omega::BacklightState,
    /// The monitors the compositor is driving.
    Monitors: omega_proto::omega::DisplayState,
    /// The workspaces, and which is being looked at.
    Workspaces: omega_proto::omega::WorkspacesState,
    /// What has focus.
    Window: omega_proto::omega::WindowState,
    /// How the machine is being typed at.
    Input: omega_proto::omega::InputState,

    /// The wall clock, in the machine's own zone.
    Clock: omega_proto::omega::TimeState,
    /// Whether anybody is using the machine.
    Idle: omega_proto::omega::IdleState,
    /// What the machine is doing with itself.
    System: omega_proto::omega::SystemState,
    /// Where it keeps things.
    Disk: omega_proto::omega::DiskState,

    /// The supervisor's report on every unit it runs.
    Units: omega_proto::omega::UnitsState,
}

/// Declares a state handle: its capability, and the accessor that reads the
/// current value.
///
/// A macro because every handle is the same three lines of wiring around a
/// different topic, and a handle that got them subtly wrong would be a
/// plugin reading someone else's state. The topic is not one of the three:
/// it comes from the value type, which is where `state.proto` already bound
/// it, so a handle cannot name a topic other than its own.
macro_rules! reads {
    ($handle:ident, $value:ty) => {
        impl $crate::wiring::Wiring for $handle {
            const TOPICS: &'static [omega_proto::SystemTopic] =
                &[<$value as omega_proto::TopicValue>::TOPIC];
            const CAPABILITIES: &'static [omega_proto::omega::Capability] =
                &[omega_proto::omega::Capability::StateRead];

            fn build(context: &$crate::context::Context) -> Self {
                Self {
                    context: context.clone(),
                }
            }
        }

        impl $crate::wiring::Reads for $handle {}

        impl $handle {
            /// Whether there is a reading right now.
            ///
            /// False on a machine that has no such device, and false while a
            /// broker that reports it is down — a widget branches on having a
            /// reading, not on why it has none. The runtime holds a plugin's
            /// first render until the daemon has spoken about every topic it
            /// declared, which includes saying there is nothing to report, so
            /// this is answerable rather than a wait.
            pub fn has_reading(&self) -> bool {
                self.read().is_some()
            }

            /// The whole reading, as the daemon published it.
            ///
            /// The floor under every handle. A topic with typed accessors has
            /// better ways to be asked — `charge()` gives a `Percent` where
            /// this gives the wire's `f64` — but nothing is unreachable for
            /// want of somebody writing them.
            pub fn get(&self) -> Option<$value> {
                self.read()
            }

            fn read(&self) -> Option<$value> {
                self.context.topic::<$value>()
            }
        }
    };
}

pub(crate) use reads;

#[cfg(test)]
mod tests {
    use super::HANDLED;
    use omega_proto::SystemTopic;

    #[test]
    fn every_topic_can_be_read() {
        // The mirror of `omega-brokers`' coverage test, which asks whether
        // anything *fills* a topic. This asks whether anything can *read* one.
        //
        // Both directions matter and only one was checked: thirteen topics
        // were brokered, coalesced and replicated into units that had no way
        // to name them, and every gate stayed green the whole time.
        for topic in SystemTopic::ALL {
            assert!(
                HANDLED.contains(topic),
                "the ontology declares {topic} and no handle reads it — add one \
                 to `handles!`, or a unit cannot see a topic the daemon is \
                 publishing to it"
            );
        }
    }

    #[test]
    fn no_topic_is_read_by_two_handles() {
        // Two handles on one topic would be two names for one reading, and a
        // manifest listing the topic twice.
        let mut seen = HANDLED.to_vec();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), HANDLED.len());
    }
}
