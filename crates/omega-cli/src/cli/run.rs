//! `omega run`: invoke a command through its configured provider.

use omega_proto::CommandId;
use omega_proto::omega::{Value, value};

use crate::operator::Operator;
use crate::ui::{Paint, Step, Ui};

/// Invoke a registered command using its provider's granted capabilities.
#[derive(Debug, clap::Args)]
pub struct RunCmd {
    /// The command ID declared by its implementation.
    #[arg(value_name = "COMMAND")]
    pub command_id: CommandId,
    /// Arguments decoded by the command: e.g. 40%, balanced, true, or text.
    #[arg(allow_negative_numbers = true)]
    pub args: Vec<String>,
}

impl RunCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let args = self
            .args
            .iter()
            .map(|argument| Self::text(argument))
            .collect();

        match Operator::new().run(&self.command_id, args).await? {
            // The answer is data: it goes to stdout undecorated, so that
            // `omega run battery.level | jq` gets a value and not a report.
            Some(answer) => ui.line(Self::render(&answer)),
            None => ui.step(Step::Done, Paint::name(self.command_id.to_string())),
        }
        Ok(())
    }

    // The command owns its input type; guessing here would corrupt text inputs.
    fn text(argument: &str) -> Value {
        Value {
            kind: Some(value::Kind::StringValue(argument.to_string())),
        }
    }

    /// What a command answered, in one line.
    fn render(answer: &Value) -> String {
        match answer.kind.as_ref() {
            Some(value::Kind::StringValue(text)) => text.clone(),
            Some(value::Kind::IntValue(number)) => number.to_string(),
            Some(value::Kind::DoubleValue(number)) => number.to_string(),
            Some(value::Kind::BoolValue(flag)) => flag.to_string(),
            other => format!("{other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::{Cli, Command};
    use clap::Parser;

    #[test]
    fn run_preserves_arguments_for_type_directed_decoding() {
        for argument in ["-42", "40%", "001", "true", "two words"] {
            let cli = Cli::try_parse_from(["omega", "run", "example.set", argument]).unwrap();
            let Command::Run(run) = cli.command else {
                panic!("expected run")
            };
            assert_eq!(run.args, [argument]);
        }
    }
}
