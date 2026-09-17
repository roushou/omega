//! Authorize protocol actions and route them to daemon, plugin, or broker handlers.

use omega_proto::omega::{CallCommand, InvokePlugin, RunCommand, action, invoke};
use omega_proto::{ActionKind, PluginName};
use omega_proto::{CommandAnswer, Refusal};

use crate::refusal::Refusable;

use crate::authorization::Grants;
use crate::broker::Brokerage;
use crate::plugins::PluginRegistry;
use crate::refusal::RefusableResult;

/// Dispatch actions to daemon handlers, plugin commands, or subsystem brokers.
#[derive(Debug)]
pub struct Actions {
    plugins: PluginRegistry,
    brokers: Brokerage,
}

impl Actions {
    pub fn new(plugins: PluginRegistry, brokers: Brokerage) -> Self {
        Self { plugins, brokers }
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
    pub async fn perform(&self, action: &action::Kind) -> Result<CommandAnswer, Refusal> {
        action.validate().or_refuse()?;
        match action {
            // The daemon's own: spawning a process is not brokering a
            // subsystem, and routing between plugins is its own job.
            action::Kind::RunCommand(run) => Self::run(run),
            action::Kind::InvokePlugin(invoke) => self.invoke_plugin(invoke).await,
            other => self.broker(other).await,
        }
    }

    /// Dispatch to the responsible broker. Return Unimplemented when no broker claims
    /// the action; propagate execution failures from a registered broker.
    async fn broker(&self, action: &action::Kind) -> Result<CommandAnswer, Refusal> {
        match self.brokers.act(action).await {
            None => Err(Refusal::unimplemented(format!(
                "{} is not performed by this daemon",
                ActionKind::of(action).name()
            ))),
            Some(Ok(())) => Ok(CommandAnswer::Acknowledged),
            Some(Err(error)) => Err(error.refusal()),
        }
    }

    /// Validate the target command against the manifest before dispatching it.
    async fn invoke_plugin(&self, call: &InvokePlugin) -> Result<CommandAnswer, Refusal> {
        let plugin = PluginName::try_from(call.plugin.clone())
            .map_err(|e| Refusal::invalid(e.to_string()))?;

        self.declares_command(&plugin, &call.command)?;

        let outcome = self
            .plugins
            .request(
                &plugin,
                invoke::Op::CallCommand(CallCommand {
                    command: call.command.clone(),
                    args: call.args.clone(),
                }),
            )
            .await
            .or_refuse()?;

        CommandAnswer::try_from(outcome)
    }

    /// A plugin serves the commands its manifest declares, and no others.
    fn declares_command(&self, plugin: &PluginName, command: &str) -> Result<(), Refusal> {
        let Some(entry) = self.plugins.manifest(plugin) else {
            return Err(Refusal::invalid(format!(
                "{plugin} is not a plugin of this build"
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
                "{plugin} declares no command surfaces"
            ))),
            false => Err(Refusal::invalid(format!(
                "{plugin} declares no command {command:?}; it has: {}",
                commands.join(", ")
            ))),
        }
    }

    /// Spawn a shell command without waiting for its exit.
    fn run(run: &RunCommand) -> Result<CommandAnswer, Refusal> {
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

        Ok(CommandAnswer::Acknowledged)
    }
}
