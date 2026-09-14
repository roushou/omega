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
//! # async fn example(volume: &omega::platform::audio::Volume) -> omega::Result<()> {
//! volume.set(omega::Percent::whole(50)).await?;
//! # Ok(()) }
//! # }
//! ```

mod completion;
pub(crate) mod queue;
pub use completion::{Completion, Effect, EffectError, Receipt, Submission};
