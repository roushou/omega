//! UI declarations, instance behavior, and presentation lifecycle.

use crate::View;
use crate::wiring::Wired;

/// A declarative UI surface backed by read-only dependencies.
///
/// The first render waits until each required topic has been reported. An absent
/// reading counts as reported. Use [`Optional`] to
/// render before a dependency is ready. Unwritten records use their defaults.
///
/// Subsequent dependency changes trigger rendering. Identical views are not
/// published again. Effects belong in commands, reactions, or stateful behavior.
pub trait Surface: Wired {
    fn render(&self) -> View;
}

mod events;
mod stateful;
mod task;
pub(crate) use events::Decoder;
pub use events::Events;
pub use stateful::StatefulSurface;
pub use task::Task;

pub(crate) mod instance;

mod optional;
pub use optional::Optional;

mod text;
pub use text::{TextEdit, TextValue};

mod lifecycle;
pub use lifecycle::Lifecycle;

mod presentation;
pub use presentation::Presentation;

mod reference;
pub use reference::{SurfaceIdentity, SurfaceRef};
