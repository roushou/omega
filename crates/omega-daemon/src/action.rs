//! Actions: the closed taxonomy of what a unit may make the machine do.
//!
//! Two tables, declared once. [`ActionKind`] names every action in
//! `action.proto` — exhaustively, so the schema growing is a compile error
//! here until the new action's cost is stated — and `COST` says which
//! capability each one requires.
//!
//! Authorization is complete even where implementation is not: an action the
//! daemon cannot perform yet is still refused for the right reason first, so
//! a unit can never be granted something by the accident of a missing
//! handler.

use omega_proto::Refusal;
use omega_proto::UnitName;
use omega_proto::omega::{CallCommand, Capability, InvokeUnit, RunCommand, action, invoke, result};

use crate::refusal::RefusableResult;
use crate::session::admission::Grants;
use crate::session::dispatch::Response;
use crate::units::UnitTable;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    LaunchApp,
    RunCommand,
    SetSetting,
    ToggleSetting,
    SwitchWorkspace,
    MoveToWorkspace,
    MoveToMonitor,
    CloseWindow,
    Lock,
    Sleep,
    Hibernate,
    Reboot,
    Shutdown,
    Screenshot,
    MediaKey,
    SetVolume,
    SetBacklight,
    Notify,
    InvokeUnit,
    ToggleFloating,
    ToggleFullscreen,
}

impl ActionKind {
    pub fn of(action: &action::Kind) -> Self {
        match action {
            action::Kind::LaunchApp(_) => Self::LaunchApp,
            action::Kind::RunCommand(_) => Self::RunCommand,
            action::Kind::SetSetting(_) => Self::SetSetting,
            action::Kind::ToggleSetting(_) => Self::ToggleSetting,
            action::Kind::SwitchWorkspace(_) => Self::SwitchWorkspace,
            action::Kind::MoveToWorkspace(_) => Self::MoveToWorkspace,
            action::Kind::MoveToMonitor(_) => Self::MoveToMonitor,
            action::Kind::CloseWindow(_) => Self::CloseWindow,
            action::Kind::Lock(_) => Self::Lock,
            action::Kind::Sleep(_) => Self::Sleep,
            action::Kind::Hibernate(_) => Self::Hibernate,
            action::Kind::Reboot(_) => Self::Reboot,
            action::Kind::Shutdown(_) => Self::Shutdown,
            action::Kind::Screenshot(_) => Self::Screenshot,
            action::Kind::MediaKey(_) => Self::MediaKey,
            action::Kind::SetVolume(_) => Self::SetVolume,
            action::Kind::SetBacklight(_) => Self::SetBacklight,
            action::Kind::Notify(_) => Self::Notify,
            action::Kind::InvokeUnit(_) => Self::InvokeUnit,
            action::Kind::ToggleFloating(_) => Self::ToggleFloating,
            action::Kind::ToggleFullscreen(_) => Self::ToggleFullscreen,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::LaunchApp => "LaunchApp",
            Self::RunCommand => "RunCommand",
            Self::SetSetting => "SetSetting",
            Self::ToggleSetting => "ToggleSetting",
            Self::SwitchWorkspace => "SwitchWorkspace",
            Self::MoveToWorkspace => "MoveToWorkspace",
            Self::MoveToMonitor => "MoveToMonitor",
            Self::CloseWindow => "CloseWindow",
            Self::Lock => "Lock",
            Self::Sleep => "Sleep",
            Self::Hibernate => "Hibernate",
            Self::Reboot => "Reboot",
            Self::Shutdown => "Shutdown",
            Self::Screenshot => "Screenshot",
            Self::MediaKey => "MediaKey",
            Self::SetVolume => "SetVolume",
            Self::SetBacklight => "SetBacklight",
            Self::Notify => "Notify",
            Self::InvokeUnit => "InvokeUnit",
            Self::ToggleFloating => "ToggleFloating",
            Self::ToggleFullscreen => "ToggleFullscreen",
        }
    }

    /// The capability this action costs. `None` is not "free": it means the
    /// action affects only the unit's own surfaces, and the daemon still has
    /// to be able to perform it.
    pub fn cost(self) -> Option<Capability> {
        match self {
            Self::RunCommand | Self::LaunchApp => Some(Capability::Spawn),
            Self::Lock | Self::Sleep | Self::Hibernate | Self::Reboot | Self::Shutdown => {
                Some(Capability::SystemControl)
            }
            Self::MediaKey => Some(Capability::Media),
            Self::SetVolume => Some(Capability::Audio),
            Self::SetBacklight => Some(Capability::Backlight),
            Self::Notify => Some(Capability::Notify),
            Self::Screenshot => Some(Capability::Screenshot),
            Self::SetSetting | Self::ToggleSetting => Some(Capability::SystemControl),
            // Making another unit run its own code is making code run.
            Self::InvokeUnit => Some(Capability::Spawn),
            Self::SwitchWorkspace
            | Self::MoveToWorkspace
            | Self::MoveToMonitor
            | Self::CloseWindow
            | Self::ToggleFloating
            | Self::ToggleFullscreen => None,
        }
    }
}

/// Performing actions the daemon knows how to perform.
///
/// Some are the daemon's own doing; some are a request to a unit, which is
/// why this holds the way to reach one.
#[derive(Debug)]
pub struct Actions {
    units: UnitTable,
}

impl Actions {
    pub fn new(units: UnitTable) -> Self {
        Self { units }
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
            action::Kind::RunCommand(run) => Self::run(run),
            action::Kind::InvokeUnit(invoke) => self.invoke_unit(invoke).await,
            other => Err(Refusal::unimplemented(format!(
                "{} is not performed by this daemon",
                ActionKind::of(other).name()
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
