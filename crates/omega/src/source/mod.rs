//! What a plugin can read.
//!
//! One handle per topic the daemon replicates. Holding one is the whole
//! declaration: the field's type names the topic, asks for permission to read
//! it, and hands back values in units that print themselves.
//!
//! Every handle reads through to whatever the daemon last published, so a
//! render is always of the current picture and never of a snapshot taken at
//! construction.

mod audio;
mod backlight;
mod battery;
mod clock;
mod network;
mod wifi;

pub use audio::Audio;
pub use backlight::Backlight;
pub use battery::Battery;
pub use clock::{Clock, Weekday};
pub use network::Network;
pub use wifi::{AccessPoint, Wifi};

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

            fn read(&self) -> Option<$value> {
                self.context.topic::<$value>()
            }
        }
    };
}

pub(crate) use reads;
