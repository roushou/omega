//! Public typed plugin endpoints, independent of UI instances.

use crate::wiring::Wired;
use omega_proto::IntoValue;

/// Something to be asked to do.
///
/// Called by `omega run`, by a keybind, by a button in this plugin's own
/// view, or by another plugin that was granted the right to. It runs with
/// this plugin's capabilities and nobody else's.
/// Commands run concurrently with socket processing. Shared mutable command
/// state must synchronize its own access; record updates already do so.
///
/// ```no_run
/// use omega::Command;
/// #[derive(omega::Command)]
/// struct Lock { session: omega::platform::session::Session }
/// impl Command for Lock {
///     type Input = ();
///     type Output = ();
///     async fn call(&self, _: ()) -> Result<(), omega::Error> {
///         self.session.lock().await
///     }
/// }
/// ```
pub trait Command: Wired + crate::command::CommandName {
    type Input: crate::Input;
    type Output: IntoValue + Send;
    fn call(
        &self,
        args: Self::Input,
    ) -> impl std::future::Future<Output = Result<Self::Output, crate::Error>> + Send;
}

mod args;
mod input;
mod reference;
pub use args::Args;
pub use input::Input;
pub use reference::{CommandName, CommandRef};
