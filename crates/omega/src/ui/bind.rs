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
///     const ID: &'static str = "set-volume";
///
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
///     const ID: &'static str = "join";
///
///     type Input = String;
///     type Output = ();
///     async fn call(&self, _: String) -> omega::Result<()> { Ok(()) }
/// }
/// Button::new("Join").on_press(Join);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Bind<I> {
    command: String,
    plugin: String,
    signature: Vec<u8>,
    local: u64,
    args: Vec<Value>,
    input: PhantomData<fn(I)>,
}
impl<C: Command> From<CommandRef<C>> for Bind<C::Input> {
    fn from(_: CommandRef<C>) -> Self {
        Self {
            command: C::ID.to_string(),
            plugin: "".into(),
            signature: CommandRef::<C>::INSTANCE.descriptor().signature(),
            local: 0,
            args: Vec::new(),
            input: PhantomData,
        }
    }
}
impl<I> Bind<I> {
    pub(crate) fn local(id: u64) -> Self {
        Self {
            command: String::new(),
            plugin: String::new(),
            signature: Vec::new(),
            args: Vec::new(),
            local: id,
            input: PhantomData,
        }
    }
    pub(crate) fn into_wire(self) -> WireBind {
        WireBind {
            command: self.command,
            plugin: self.plugin,
            signature: self.signature,
            local: self.local,
            args: self.args,
        }
    }
}

impl<C: Command> From<crate::command::Invocation<C>> for Bind<()> {
    fn from(invocation: crate::command::Invocation<C>) -> Self {
        let call = invocation.into_wire();
        Self {
            command: call.command,
            plugin: call.plugin,
            signature: call.signature,
            args: call.args,
            local: 0,
            input: PhantomData,
        }
    }
}
