//! Typed UI interactions targeting a local message or a declared command.
use crate::{Command, command::CommandRef};
use omega_proto::omega::{Bind as WireBind, Value};
use std::marker::PhantomData;

/// A local-message or command binding expecting an interaction of type `I`.
/// Controls accept only bindings matching the value they submit.
///
/// A boolean control cannot invoke a command expecting a percentage:
///
/// ```compile_fail
/// use omega::{Command, Percent, ui::Toggle};
/// #[derive(omega::Command)]
/// struct SetVolume {}
/// impl Command for SetVolume {
///     type Input = Percent;
///     type Output = ();
///     async fn call(&self, _: Percent) -> omega::Result<()> { Ok(()) }
/// }
/// Toggle::new(false).on_change(SetVolume);
/// ```
///
/// A button must bind the input a command requires:
///
/// ```compile_fail
/// use omega::{Command, ui::Button};
/// #[derive(omega::Command)]
/// struct Join {}
/// impl Command for Join {
///     type Input = String;
///     type Output = ();
///     async fn call(&self, _: String) -> omega::Result<()> { Ok(()) }
/// }
/// Button::new("Join").on_press(Join);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Bind<I> {
    command: String,
    local: u64,
    args: Vec<Value>,
    input: PhantomData<fn(I)>,
}
impl<C: Command> From<CommandRef<C>> for Bind<C::Input> {
    fn from(_: CommandRef<C>) -> Self {
        Self {
            command: C::NAME.to_string(),
            local: 0,
            args: Vec::new(),
            input: PhantomData,
        }
    }
}
impl<I> Bind<I> {
    pub(crate) fn command(name: &str, args: Vec<Value>) -> Self {
        Self {
            command: name.into(),
            local: 0,
            args,
            input: PhantomData,
        }
    }

    pub(crate) fn local(id: u64) -> Self {
        Self {
            command: String::new(),
            args: Vec::new(),
            local: id,
            input: PhantomData,
        }
    }
    pub(crate) fn into_wire(self) -> WireBind {
        WireBind {
            command: self.command,
            local: self.local,
            args: self.args,
        }
    }
}
