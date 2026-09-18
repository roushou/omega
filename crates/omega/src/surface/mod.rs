//! UI declarations, instance behavior, and presentation lifecycle.

use crate::View;
use crate::wiring::Wired;

/// A UI declaration with an instance-owned model and serialized local messages.
///
/// Required readings delay the first render until reported, including reported
/// absence. [`Optional`] permits rendering before its dependency is reported.
/// Dependency changes and local updates invalidate the view; identical trees are
/// not published again.
///
/// Use `Model = ()`, `Message = std::convert::Infallible`, and `Effects = ()`
/// when no local state, messages, or effects are needed. Implement `update` with
/// `match message {}` for `Infallible`; registered commands remain usable in controls.
///
/// Derive `Surface` on the declaration: its fields may only read. Effect handles
/// are constructed separately and passed only to behavior, never to `render`.
/// Models and messages remain ordinary Rust values without serialization.
///
/// ```
/// use omega::{Surface, View, surface::{Events, Task}, ui::Button};
/// #[derive(omega::Surface)]
/// struct Counter {}
/// impl Surface for Counter {
///     type Model = u32;
///     type Message = ();
///     type Effects = ();
///     fn render(&self, count: &u32, events: &Events<()>) -> View {
///         Button::new(count).on_press(events.send(())).into()
///     }
///     fn update(&self, count: &mut u32, _: (), _: &()) -> Task<()> {
///         *count += 1;
///         Task::none()
///     }
/// }
/// let plugin = omega::plugin!().surface(Counter);
/// assert!(plugin.manifest().unwrap().commands.is_empty());
/// ```
///
/// Effects cannot be fields of the render declaration:
///
/// ```compile_fail
/// #[derive(omega::Surface)]
/// struct Search { volume: omega::platform::audio::Volume }
/// ```
///
/// Every surface must implement message handling:
///
/// ```compile_fail,E0046
/// use omega::{Surface, View, surface::Events};
/// #[derive(omega::Surface)]
/// struct MissingHandler;
/// impl Surface for MissingHandler {
///     type Model = ();
///     type Message = ();
///     type Effects = ();
///     fn render(&self, _: &(), _: &Events<()>) -> View { View::empty() }
/// }
/// ```
///
/// An empty handler stops compiling when messages become possible:
///
/// ```compile_fail,E0004
/// use omega::{Surface, View, surface::{Events, Task}};
/// #[derive(omega::Surface)]
/// struct Counter;
/// enum Message { Increment }
/// impl Surface for Counter {
///     type Model = u32;
///     type Message = Message;
///     type Effects = ();
///     fn render(&self, _: &u32, _: &Events<Message>) -> View { View::empty() }
///     fn update(&self, _: &mut u32, message: Message, _: &()) -> Task<Message> {
///         match message {}
///     }
/// }
/// ```
pub trait Surface: Wired {
    /// Local state, default-initialized once per instance. Use `()` when unused.
    type Model: Default + Send + Sync + 'static;
    /// Local events and task results. Use `Infallible` when no messages can occur.
    type Message: Send + 'static;
    /// Behavior dependencies, constructed separately from render-side readings.
    type Effects: Wired;

    /// Initialize instance subscriptions before mounting or rendering.
    /// Failures refuse instance construction and release its handles.
    fn initialize(&mut self, _model: &mut Self::Model) -> crate::Result<()> {
        Ok(())
    }

    /// Describe the current view without performing effects.
    fn render(&self, model: &Self::Model, events: &Events<Self::Message>) -> View;
    /// Apply one local message and return any asynchronous work to schedule.
    fn update(
        &self,
        model: &mut Self::Model,
        message: Self::Message,
        effects: &Self::Effects,
    ) -> Task<Self::Message>;
    /// Close cancels managed work before this hook. Hidden instances retain work.
    fn lifecycle(
        &self,
        _model: &mut Self::Model,
        _event: Lifecycle,
        _effects: &Self::Effects,
    ) -> Task<Self::Message> {
        Task::none()
    }

    /// Initialize the model once, before its first render, even while readings are pending.
    fn mounted(&self, _model: &mut Self::Model, _effects: &Self::Effects) -> Task<Self::Message> {
        Task::none()
    }
}

mod events;
mod task;
pub(crate) use events::Decoder;
pub use events::{BindingError, Events};
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
