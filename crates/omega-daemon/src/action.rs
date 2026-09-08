//! Performing actions, and refusing the ones a unit may not ask for.
//!
//! The taxonomy and its costs are [`ActionKind`], declared in `omega-proto`
//! because a broker names the kinds it serves. This is the half that decides
//! whether a caller may, and then does the ones the daemon itself performs.

use omega_proto::Refusal;
use omega_proto::UnitName;
use omega_proto::omega::{CallCommand, InvokeUnit, RunCommand, action, invoke, result};

use crate::broker::Brokerage;
use crate::refusal::RefusableResult;
use crate::session::admission::Grants;
use crate::session::dispatch::Response;
use crate::units::UnitTable;

pub use omega_proto::ActionKind;

/// Performing actions the daemon knows how to perform.
///
/// Three kinds: the daemon's own doing, a request to a unit, and a request to
/// the broker that owns the subsystem — which is why this holds the way to
/// reach both.
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
        match action {
            // The daemon's own: spawning a process is not brokering a
            // subsystem, and routing between units is its own job.
            action::Kind::RunCommand(run) => Self::run(run),
            action::Kind::InvokeUnit(invoke) => self.invoke_unit(invoke).await,
            other => self.broker(other).await,
        }
    }

    /// Hand the action to whichever broker owns the subsystem.
    ///
    /// A kind nothing claims is `UNIMPLEMENTED` — the daemon cannot do it,
    /// and saying so is what keeps a granted capability from standing in for
    /// a handler that does not exist. A broker that *did* claim it and failed
    /// is a different answer: the subsystem is there and said no.
    async fn broker(&self, action: &action::Kind) -> Result<Response, Refusal> {
        match self.brokers.act(action).await {
            None => Err(Refusal::unimplemented(format!(
                "{} is not performed by this daemon",
                ActionKind::of(action).name()
            ))),
            Some(Ok(())) => Ok(Response::Ok),
            Some(Err(error)) => Err(Refusal::unimplemented(format!(
                "{} could not be performed: {error}",
                ActionKind::of(action).name()
            ))),
        }
    }

    /// Call a unit's command surface.
    ///
    /// The daemon checks that the unit declares the command before asking for
    /// it: a typo should be answered here, with the list of what does exist,
    /// rather than by a unit that has to invent its own error for it.
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
            .surfaces
            .iter()
            .filter(|surface| {
                matches!(surface.kind(), Ok(omega_proto::omega::SurfaceKind::Command))
            })
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

    /// The shell escape hatch. Spawned and left alone: an action is a request
    /// to do something, not a request to wait for it, and a unit that blocks
    /// the daemon on a slow command would take the desktop with it.
    fn run(run: &RunCommand) -> Result<Response, Refusal> {
        if run.command.trim().is_empty() {
            return Err(Refusal::invalid("RunCommand carries no command"));
        }

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
