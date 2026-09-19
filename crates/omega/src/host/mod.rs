//! Executable providers for reusable commands.
//!
//! Registration declares capabilities without constructing dependencies. The
//! daemon selects lifetime and execution policy; importing a host starts nothing.

use crate::Command;
use crate::program::{Program, ProgramKind, registration::CommandEntry};
pub use omega_proto::host::{ExecutionPolicy, HostId, HostPolicy, Lifetime, StartPolicy};
mod deployment;
pub use deployment::CommandHostDeployment;

/// An executable registering commands without UI surfaces or reactions.
///
/// Register the declaration in the system document using default deployment
/// settings, or call [`Self::deployment`] to configure its lifetime and execution
/// limits. Importing the library or inspecting its manifest starts no process.
///
/// ```no_run
/// use omega::{Command, host::CommandHost};
/// struct Echo;
/// impl omega::command::Construct for Echo {
///     type Dependencies = ();
///     fn construct((): ()) -> Self { Self }
/// }
///
/// impl Command for Echo {
///     type Input = String;
///     type Output = String;
///     const ID: &'static str = "example.echo";
///     async fn call(&self, text: String) -> omega::Result<String> { Ok(text) }
/// }
/// CommandHost::new("example", "1.0.0").command::<Echo>().run()?;
/// # Ok::<(), omega::Error>(())
/// ```
#[derive(Debug)]
pub struct CommandHost {
    program: Program,
}

impl CommandHost {
    /// Declare a provider identity independent of its package name.
    pub fn new(id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            program: Program::new(id, version, ProgramKind::Commands),
        }
    }

    /// Register a typed command endpoint and its dependency requirements.
    pub fn command<C: Command>(mut self) -> Self {
        self.program
            .registrations
            .commands
            .push(CommandEntry::of::<C>(C::ID.to_string()));
        self
    }

    /// Select how the daemon should run this declaration through the system document.
    pub fn deployment(self) -> CommandHostDeployment {
        self.into()
    }

    /// Describe commands and requirements without constructing handlers.
    pub fn manifest(&self) -> crate::Result<omega_proto::Manifest> {
        self.program.manifest()
    }

    /// Describe this executable or start a runtime and connect to its supervising daemon.
    pub fn run(self) -> crate::Result<()> {
        self.program.prepare()?.run()
    }

    /// Serve using the caller's asynchronous runtime. Requires a daemon host assignment.
    pub async fn serve(self) -> crate::Result<()> {
        self.program.prepare()?.serve().await
    }
}
