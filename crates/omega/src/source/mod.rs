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
mod battery;
mod network;

pub use audio::Audio;
pub use battery::Battery;
pub use network::Network;

/// Declares a state handle: its topic, its capability, and the accessor that
/// reads the current value.
///
/// A macro because every handle is the same three lines of wiring around a
/// different topic, and a handle that got them subtly wrong would be a
/// plugin reading someone else's state.
macro_rules! reads {
    ($handle:ident, $topic:ident, $value:ty) => {
        impl $crate::wiring::Wiring for $handle {
            const TOPICS: &'static [omega_wire::SystemTopic] = &[omega_wire::SystemTopic::$topic];
            const CAPABILITIES: &'static [omega_wire::omega::Capability] =
                &[omega_wire::omega::Capability::StateRead];

            fn build(context: &$crate::context::Context) -> Self {
                Self {
                    context: context.clone(),
                }
            }
        }

        impl $crate::wiring::Reads for $handle {}

        impl $handle {
            /// Whether the daemon has published this topic at all.
            ///
            /// Rarely needed: the runtime holds a plugin's first render until
            /// every topic it declared has a value.
            pub fn is_known(&self) -> bool {
                self.read().is_some()
            }

            fn read(&self) -> Option<$value> {
                self.context.topic::<$value>()
            }
        }
    };
}

pub(crate) use reads;
