//! Typed command endpoints, independent of providers and UI instances.

/// A reusable typed operation callable from UI bindings, schedules, and other commands.
///
/// Register the command in one plugin or command host. `ID` names the operation
/// independently of that provider. Each invocation constructs a fresh handler
/// after required readings initialize. Shared state must be an explicit dependency.
/// Host policy controls concurrency; commands registered in plugins run concurrently.
/// Derive `omega::Command` to construct dependencies from fields, or implement
/// [`Construct`] explicitly without a macro.
///
/// ```no_run
/// use omega::{Command, platform::session::Session};
///
/// #[derive(omega::Command)]
/// struct Lock { session: Session }
///
/// impl Command for Lock {
///     type Input = ();
///     type Output = ();
///
///     const ID: &'static str = "session.lock";
///
///     async fn call(&self, _: ()) -> omega::Result<()> {
///         self.session.lock().await
///     }
/// }
/// ```
pub trait Command: Construct {
    const ID: &'static str;
    type Input: crate::Input;
    type Output: CommandValue;
    const DESCRIPTION: &'static str = "";
    fn call(
        &self,
        args: Self::Input,
    ) -> impl std::future::Future<Output = Result<Self::Output, crate::Error>> + Send;
}

mod construct;
pub use construct::Construct;

mod args;
mod input;
mod reference;
pub use crate::wiring::Wired as Dependencies;
pub use args::Args;
pub use input::Input;
pub use reference::CommandRef;

mod value;
pub use omega_proto::omega::command_type::Kind as CommandTypeKind;
pub use omega_proto::omega::{CommandEndpoint, CommandField, CommandType};
pub use omega_proto::{CommandAddress, CommandId};
pub use value::CommandValue;

mod caller;
pub use caller::{Caller, Invocation};

mod catalogue;
pub use catalogue::{Available, Commands};
