use super::{Events, Task, Wired};
use crate::View;

/// An instance-owned model with serialized local messages.
///
/// Derive `Surface` on the declaration: its fields may only read. Effect handles
/// are constructed separately and passed only to behavior, never to `render`.
/// Models and messages remain ordinary Rust values without serialization.
///
/// ```
/// use omega::{StatefulSurface, View, surface::{Events, Task}, ui::Button};
/// #[derive(omega::Surface)]
/// struct Counter {}
/// impl StatefulSurface for Counter {
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
/// let plugin = omega::plugin!().stateful(Counter);
/// assert!(plugin.manifest().unwrap().commands.is_empty());
/// ```
///
/// Effects cannot be fields of the render declaration:
///
/// ```compile_fail
/// #[derive(omega::Surface)]
/// struct Search { volume: omega::platform::audio::Volume }
/// ```
pub trait StatefulSurface: Wired {
    type Model: Default + Send + Sync + 'static;
    type Message: Send + 'static;
    type Effects: Wired;

    fn render(&self, model: &Self::Model, events: &Events<Self::Message>) -> View;
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
        _event: super::Lifecycle,
        _effects: &Self::Effects,
    ) -> Task<Self::Message> {
        Task::none()
    }

    /// Initialize the model once, before its first render, even while readings are pending.
    fn mounted(&self, _model: &mut Self::Model, _effects: &Self::Effects) -> Task<Self::Message> {
        Task::none()
    }
}

impl Wired for () {
    fn topics() -> Vec<omega_proto::SystemTopic> {
        Vec::new()
    }
    fn capabilities() -> Vec<omega_proto::omega::Capability> {
        Vec::new()
    }
    fn build(_: &crate::runtime::context::Context, _: &omega_proto::Values) -> Self {}
}
