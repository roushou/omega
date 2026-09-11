//! Completion and failure semantics for external actions.
//!
//! Domain controls return [`Effect`]. Await it to observe the terminal result,
//! or obtain a [`Receipt`] for explicit completion handling in reactions.
//! Detached failures end the runtime. At most 64 effects and 8 MiB of logical
//! payloads are admitted; timed-out sent work retains admission until a terminal
//! reply or disconnect.
//!
//! ```no_run
//! # struct Example;
//! # impl Example {
//! # async fn example(volume: &omega::audio::Volume) -> omega::Result<()> {
//! volume.set(omega::Percent::whole(50)).await?;
//! # Ok(()) }
//! # }
//! ```

pub(crate) mod brightness;
mod completion;
pub(crate) mod queue;
pub use completion::{Completion, Effect, EffectError, Receipt, Submission};
pub(crate) mod notify;
pub(crate) mod power_profile;
pub(crate) mod session;
pub(crate) mod shell;
pub(crate) mod volume;
pub(crate) mod wifi;

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
