//! UI declarations, instance behavior, and presentation lifecycle.

use crate::View;
use crate::wiring::Wired;

/// Something to draw.
///
/// `render` is called once the topics its fields declare have arrived, and
/// again when its dependencies or model are invalidated. Required readings gate
/// each instance independently; `Optional<R>` removes a dependency’s startup gate. Explicit absence counts as a
/// report, and unwritten records have defaults. A requested instance awaiting its
/// topics answers with an empty view and publishes once ready.
///
/// The runtime suppresses identical trees per instance before sending them.
/// Rendering holds only readings; effects are excluded by the `Reads` bound.
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
