//! The escape hatch.

use omega_proto::omega::{LaunchApp, RunCommand, action};

use crate::context::Context;
use crate::effect::does;

/// Permission to start something.
///
/// The costliest field a plugin can hold: everything else names what it
/// changes, and this names nothing. Prefer a typed effect where one exists —
/// [`Session`] locks the screen, [`Volume`] changes the volume — and reach
/// for this when the machine has no word for what you want.
///
/// [`Session`]: crate::Session
/// [`Volume`]: crate::Volume
#[derive(Debug)]
pub struct Shell {
    context: Context,
}

does!(Shell, Spawn);

impl Shell {
    /// Run a command line.
    pub fn run(&self, command: impl Into<String>) {
        self.act(action::Kind::RunCommand(RunCommand {
            command: command.into(),
        }));
    }

    /// Launch a desktop entry by id — `"firefox.desktop"`. Typed, and no
    /// shell involved, so prefer it over [`run`].
    ///
    /// [`run`]: Self::run
    pub fn launch(&self, desktop_id: impl Into<String>) {
        self.act(action::Kind::LaunchApp(LaunchApp {
            desktop_id: desktop_id.into(),
            args: Vec::new(),
        }));
    }
}
