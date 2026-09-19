//! Typed command identity and fully bound invocations.
use super::{CommandValue, Invocation};
use crate::{Command, Input};
use std::marker::PhantomData;

/// A reference to a command, without its effects or runtime instance.
/// `#[derive(Command)]` provides a value with the command's type name.
///
/// ```
/// use omega::{Command, Percent};
/// use omega::ui::Slider;
/// #[derive(omega::Command)]
/// struct SetVolume { volume: omega::platform::audio::Volume }
/// impl Command for SetVolume {
///     const ID: &'static str = "set-volume";
///
///     type Input = Percent;
///     type Output = ();
///     async fn call(&self, value: Percent) -> omega::Result<()> {
///         self.volume.set(value).await
///     }
/// }
/// Slider::new(Percent::whole(50)).on_change(SetVolume);
/// ```
pub struct CommandRef<C>(PhantomData<fn() -> C>);
impl<C> Copy for CommandRef<C> {}
impl<C> Clone for CommandRef<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> std::fmt::Debug for CommandRef<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CommandRef")
    }
}

impl<C> CommandRef<C> {
    pub const fn new() -> Self {
        Self::INSTANCE
    }
    #[doc(hidden)]
    pub const INSTANCE: Self = Self(PhantomData);
}

impl<C: Command> CommandRef<C> {
    pub fn descriptor(self) -> omega_proto::omega::CommandEndpoint {
        omega_proto::omega::CommandEndpoint {
            id: C::ID.into(),
            input: Some(C::Input::shape()),
            output: Some(C::Output::shape()),
            description: C::DESCRIPTION.into(),
        }
    }

    /// The explicitly declared command identity.
    pub fn name(self) -> &'static str {
        C::ID
    }

    /// Bind a complete input for a button to submit.
    pub fn with(self, input: C::Input) -> Invocation<C> {
        Invocation::new(input)
    }
}

impl<C> Default for CommandRef<C> {
    fn default() -> Self {
        Self::new()
    }
}
