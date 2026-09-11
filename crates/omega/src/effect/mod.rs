//! What a plugin can do.
//!
//! One handle per thing a plugin can change about the machine, each carrying
//! the capability it costs. Holding one is asking for that capability; the
//! daemon reads the manifest the fields produced and grants exactly that.
//!
//! None of these may be held by a widget. Rendering happens on every state
//! change and identical trees are dropped, so an effect there fires on every
//! percent the battery moves — the compiler says so, by way of [`Does`].
//!
//! Await an effect to observe its terminal result. Manual receipt handling is
//! available for reactions and explicitly detached work. Detached failures end
//! the runtime. At most 64 effects and 8 MiB of logical payloads are admitted;
//! timed-out sent work retains admission until a terminal reply or disconnect.
//!
//! [`Does`]: crate::wiring::Does

mod brightness;
mod completion;
pub(crate) mod queue;
pub use completion::{Completion, Effect, EffectError, Receipt, Submission};
mod notify;
mod power_profile;
mod session;
mod shell;
mod volume;

pub use brightness::Brightness;
pub use notify::{Notification, Notify};
pub use power_profile::SetProfile;
pub use session::Session;
pub use shell::Shell;
pub use volume::Volume;

/// Declares an effect handle: what it costs, and how it is built.
macro_rules! does {
    ($handle:ident, $capability:ident) => {
        impl $crate::wiring::Wiring for $handle {
            const CAPABILITIES: &'static [omega_proto::omega::Capability] =
                &[omega_proto::omega::Capability::$capability];

            fn build(context: &$crate::context::Context) -> Self {
                Self {
                    context: context.clone(),
                }
            }
        }

        impl $crate::wiring::Does for $handle {}

        impl $handle {
            /// Queue one action for the daemon.
            fn act(&self, kind: omega_proto::omega::action::Kind) -> $crate::effect::Effect {
                $crate::effect::Effect::new(self.context.act(omega_proto::omega::invoke::Op::Act(
                    omega_proto::omega::Act {
                        action: Some(omega_proto::omega::Action { kind: Some(kind) }),
                    },
                )))
            }
        }
    };
}

pub(crate) use does;
