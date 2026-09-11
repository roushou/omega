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
/// [`Session`]: crate::session::Session
/// [`Volume`]: crate::audio::Volume
#[derive(Debug)]
pub struct Shell {
    context: Context,
}

does!(Shell, Spawn);

impl Shell {
    /// Run a command line.
    pub fn run(&self, command: impl Into<String>) -> crate::effect::Effect {
        self.act(action::Kind::RunCommand(RunCommand {
            command: command.into(),
        }))
    }

    /// Run a program with literal arguments, without interpreting them as shell syntax.
    /// Completion confirms launch, not the program's eventual exit.
    ///
    /// ```no_run
    /// # async fn example(shell: &omega::process::Shell) -> Result<(), omega::Error> {
    /// shell.run_with_args("printf", ["%s", "a name with spaces"]).await?;
    /// # Ok(()) }
    /// ```
    pub fn run_with_args(
        &self,
        program: &str,
        args: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> crate::effect::Effect {
        self.run(Self::command_line(program, args))
    }

    fn command_line(program: &str, args: impl IntoIterator<Item = impl AsRef<str>>) -> String {
        let mut command = Self::quote(program);
        for arg in args {
            command.push(' ');
            command.push_str(&Self::quote(arg.as_ref()));
        }
        command
    }

    fn quote(word: &str) -> String {
        format!("'{}'", word.replace('\'', "'\\''"))
    }

    /// Launch a desktop entry by id — `"firefox.desktop"`. Typed, and no
    /// shell involved, so prefer it over [`run`].
    ///
    /// [`run`]: Self::run
    pub fn launch(&self, desktop_id: impl Into<String>) -> crate::effect::Effect {
        self.act(action::Kind::LaunchApp(LaunchApp {
            desktop_id: desktop_id.into(),
            args: Vec::new(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_arguments_are_literal_even_when_they_contain_shell_syntax() {
        let words = ["", "a b", "a'b", "$HOME; $(printf wrong)", "line\nbreak"];
        let command = Shell::command_line("printf", std::iter::once("%s\\0").chain(words));
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .unwrap();
        assert!(output.status.success());
        let expected: Vec<u8> = words
            .iter()
            .flat_map(|word| word.bytes().chain([0]))
            .collect();
        assert_eq!(output.stdout, expected);
    }
}
