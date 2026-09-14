//! Authorize protocol actions and route them to daemon, plugin, or broker handlers.

use omega_proto::Refusal;
use omega_proto::UnitName;
use omega_proto::omega::{CallCommand, InvokeUnit, RunCommand, action, invoke, result};

use crate::refusal::Refusable;

use crate::broker::Brokerage;
use crate::refusal::RefusableResult;
use crate::session::admission::Grants;
use crate::session::dispatch::Response;
use crate::units::UnitTable;

pub use omega_proto::ActionKind;

/// Dispatch actions to daemon handlers, plugin commands, or subsystem brokers.
#[derive(Debug)]
pub struct Actions {
    units: UnitTable,
    brokers: Brokerage,
}

impl Actions {
    pub fn new(units: UnitTable, brokers: Brokerage) -> Self {
        Self { units, brokers }
    }

    /// The capability check, which happens whether or not the action is one
    /// this daemon can carry out.
    pub fn authorize(action: &action::Kind, grants: &Grants) -> Result<(), Refusal> {
        let kind = ActionKind::of(action);
        match kind.cost() {
            Some(capability) if !grants.holds(capability) => Err(Refusal::denied(format!(
                "{} requires {}",
                kind.name(),
                capability.as_str_name()
            ))),
            _ => Ok(()),
        }
    }

    /// Carry out an authorized action.
    pub async fn perform(&self, action: &action::Kind) -> Result<Response, Refusal> {
        action.validate().or_refuse()?;
        match action {
            // The daemon's own: spawning a process is not brokering a
            // subsystem, and routing between units is its own job.
            action::Kind::RunCommand(run) => Self::run(run),
            action::Kind::InvokeUnit(invoke) => self.invoke_unit(invoke).await,
            other => self.broker(other).await,
        }
    }

    /// Dispatch to the responsible broker. Return Unimplemented when no broker claims
    /// the action; propagate execution failures from a registered broker.
    async fn broker(&self, action: &action::Kind) -> Result<Response, Refusal> {
        match self.brokers.act(action).await {
            None => Err(Refusal::unimplemented(format!(
                "{} is not performed by this daemon",
                ActionKind::of(action).name()
            ))),
            Some(Ok(())) => Ok(Response::Ok),
            Some(Err(error)) => Err(error.refusal()),
        }
    }

    /// Validate the target command against the manifest before dispatching it.
    async fn invoke_unit(&self, call: &InvokeUnit) -> Result<Response, Refusal> {
        let unit =
            UnitName::parse(call.unit.clone()).map_err(|e| Refusal::invalid(e.to_string()))?;

        self.declares_command(&unit, &call.command)?;

        let outcome = self
            .units
            .request(
                &unit,
                invoke::Op::CallCommand(CallCommand {
                    command: call.command.clone(),
                    args: call.args.clone(),
                }),
            )
            .await
            .or_refuse()?;

        Ok(match outcome {
            // A command that answers with something hands it back; one that
            // just did its job says so.
            result::Outcome::Value(value) => Response::Value(value),
            _ => Response::Ok,
        })
    }

    /// A unit serves the commands its manifest declares, and no others.
    fn declares_command(&self, unit: &UnitName, command: &str) -> Result<(), Refusal> {
        let Some(entry) = self.units.manifest(unit) else {
            return Err(Refusal::invalid(format!(
                "{unit} is not a unit of this build"
            )));
        };

        let commands: Vec<&str> = entry
            .manifest
            .commands
            .iter()
            .map(|surface| surface.id.as_str())
            .collect();

        match commands.contains(&command) {
            true => Ok(()),
            false if commands.is_empty() => Err(Refusal::invalid(format!(
                "{unit} declares no command surfaces"
            ))),
            false => Err(Refusal::invalid(format!(
                "{unit} declares no command {command:?}; it has: {}",
                commands.join(", ")
            ))),
        }
    }

    /// Spawn a shell command without waiting for its exit.
    fn run(run: &RunCommand) -> Result<Response, Refusal> {
        let command = run.command.clone();
        tokio::spawn(async move {
            match tokio::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(&command)
                .status()
                .await
            {
                Ok(status) if status.success() => {
                    tracing::debug!(%command, "command finished")
                }
                Ok(status) => tracing::warn!(%command, %status, "command failed"),
                Err(e) => tracing::error!(%command, error = %e, "cannot run command"),
            }
        });

        Ok(Response::Ok)
    }
}
