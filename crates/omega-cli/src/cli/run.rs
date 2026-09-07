//! `omega run`: call a unit's command surface.

use omega_core::UnitName;
use omega_wire::omega::{Value, value};

use crate::operator::Operator;
use crate::ui::{Paint, Step, Ui};

/// Run one of a unit's declared commands.
///
/// The unit does the work with its own capabilities, which is the point: the
/// command is the unit's, and calling it is not a way to borrow powers it was
/// never granted.
#[derive(Debug, clap::Args)]
pub struct RunCmd {
    /// The unit that declares the command.
    pub unit: String,
    /// The command surface's id, as its manifest declares it.
    pub command: String,
    /// Arguments, passed through as strings.
    pub args: Vec<String>,
}

impl RunCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let unit = UnitName::parse(&self.unit)?;
        let args = self
            .args
            .iter()
            .map(|argument| Self::text(argument))
            .collect();

        match Operator::new()
            .run(unit.as_str(), &self.command, args)
            .await?
        {
            // The answer is data: it goes to stdout undecorated, so that
            // `omega run battery level | jq` gets a value and not a report.
            Some(answer) => ui.line(Self::render(&answer)),
            None => ui.step(Step::Done, Paint::name(format!("{unit} {}", self.command))),
        }
        Ok(())
    }

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
