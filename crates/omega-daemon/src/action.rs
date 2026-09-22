//! Authorize protocol actions and route them to daemon, plugin, or broker handlers.

use omega_proto::ActionKind;
use omega_proto::omega::{CaptureCommand, RunCommand, action};
use omega_proto::{CommandAnswer, IntoValue, Refusal};

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
    /// Bounded wait for a captured command.
    const CAPTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
    /// Per-stream byte limit for captured output.
    const CAPTURE_BYTES: usize = 64 * 1024;

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
            action::Kind::CaptureCommand(capture) => Self::capture(capture).await,
            action::Kind::InvokePlugin(invoke) => self.plugins.invoke_command(invoke, None).await,
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

    /// Run a shell command and return its bounded stdout to the caller.
    async fn capture(capture: &CaptureCommand) -> Result<CommandAnswer, Refusal> {
        let mut command = tokio::process::Command::new("/bin/sh");
        command.arg("-c").arg(&capture.command);
        let output = omega_host::process::Process::new(command)
            .timeout(Self::CAPTURE_TIMEOUT)
            .capture(omega_host::process::OutputLimits {
                stdout: Self::CAPTURE_BYTES,
                stderr: Self::CAPTURE_BYTES,
            })
            .await
            .map_err(|error| Refusal::unavailable(error.to_string()))?;

        if !output.status.success() {
            return Err(Refusal::unavailable(format!(
                "command exited {}",
                output.status
            )));
        }
        Ok(CommandAnswer::Value(
            String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string()
                .into_value(),
        ))
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
