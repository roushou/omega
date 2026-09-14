//! Public typed plugin endpoints, independent of UI instances.

use crate::wiring::Wired;
use omega_proto::IntoValue;

/// A typed operation callable from UI bindings, schedules, or `omega run`.
/// Commands use the owning plugin's capabilities and run concurrently.
/// Synchronize shared mutable state; record updates provide their own locking.
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
