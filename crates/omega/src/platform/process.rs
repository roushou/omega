//! Shell command and process execution.

use omega_proto::omega::{LaunchApp, RunCommand, action};

use crate::runtime::context::Context;
use crate::wiring::does;

/// Run shell commands or launch programs.
/// Prefer domain controls such as [`Session`] and [`Volume`] when available.
///
/// [`Session`]: crate::platform::session::Session
/// [`Volume`]: crate::platform::audio::Volume
#[derive(Debug)]
pub struct Shell {
    context: Context,
}

does!(Shell, Spawn);

impl Shell {
    /// Execute a command line through the shell.
    pub fn run(&self, command: impl Into<String>) -> crate::effect::Effect {
        self.act(action::Kind::RunCommand(RunCommand {
            command: command.into(),
        }))
    }

    /// Run a program with literal arguments, without interpreting them as shell syntax.
    /// Completion confirms launch, not the program's eventual exit.
    ///
    /// ```no_run
    /// # async fn example(shell: &omega::platform::process::Shell) -> Result<(), omega::Error> {
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

    /// Launch a desktop entry by ID, such as `firefox.desktop`, without shell parsing.
    pub fn launch(&self, desktop_id: impl Into<String>) -> crate::effect::Effect {
        self.act(action::Kind::LaunchApp(LaunchApp {
            activation_token: String::new(),
            desktop_id: desktop_id.into(),
            uris: Vec::new(),
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
