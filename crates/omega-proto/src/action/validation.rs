use super::ActionKind;
use crate::omega::{
    Action, Direction, PowerProfile, WindowSelector, action, media_key, move_to_workspace,
    set_backlight, set_volume, switch_workspace, window_selector,
};
use crate::{SurfaceId, UnitName};

/// A malformed action payload, independent of permissions or machine state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActionError {
    #[error("action carries no kind")]
    MissingAction,
    #[error("{action:?}.{field}: {reason}")]
    Invalid {
        action: ActionKind,
        field: &'static str,
        reason: &'static str,
    },
}

impl Action {
    /// Validate the envelope and payload without consulting the running machine.
    pub fn validate(&self) -> Result<&action::Kind, ActionError> {
        let kind = self.kind.as_ref().ok_or(ActionError::MissingAction)?;
        kind.validate()?;
        Ok(kind)
    }
}

impl action::Kind {
    /// Validate arguments. Authorization must precede this check at live ingress.
    pub fn validate(&self) -> Result<(), ActionError> {
        let input = Input(ActionKind::of(self));
        match self {
            Self::LaunchApp(app) => {
                input.text("desktop_id", &app.desktop_id)?;
                input.require(
                    "desktop_id",
                    !app.desktop_id.contains('/') && !matches!(app.desktop_id.as_str(), "." | ".."),
                    "must be a desktop-entry id, not a path",
                )?;
                for arg in &app.args {
                    input.string("args", arg)?;
                }
            }
            Self::ConnectWifi(connect) => {
                input.text("ssid", &connect.ssid)?;
                input.require("ssid", connect.ssid.len() <= 32, "must be at most 32 bytes")?;
                input.string("password", &connect.password)?;
                input.require(
                    "password",
                    connect.password.len() <= 64,
                    "must be at most 64 bytes",
                )?;
            }
            Self::DisconnectWifi(_) => {}
            Self::RunCommand(run) => input.text("command", &run.command)?,
            Self::SetSetting(set) => {
                input.text("setting_id", &set.setting_id)?;
                input.require("value", set.value.is_some(), "is required")?;
            }
            Self::ToggleSetting(toggle) => input.text("setting_id", &toggle.setting_id)?,
            Self::SwitchWorkspace(switch) => match &switch.target {
                Some(switch_workspace::Target::Index(index)) => input.index(*index)?,
                Some(switch_workspace::Target::Name(name)) => input.text("target.name", name)?,
                Some(switch_workspace::Target::Direction(direction)) => input.require(
                    "target.direction",
                    matches!(
                        Direction::try_from(*direction),
                        Ok(Direction::Next | Direction::Previous)
                    ),
                    "must be next or previous",
                )?,
                None => return input.invalid("target", "is required"),
            },
            Self::MoveToWorkspace(moving) => {
                match &moving.target {
                    Some(move_to_workspace::Target::Index(index)) => input.index(*index)?,
                    Some(move_to_workspace::Target::Name(name)) => {
                        input.text("target.name", name)?
                    }
                    None => return input.invalid("target", "is required"),
                }
                input.window(moving.window.as_ref())?;
            }
            Self::MoveToMonitor(moving) => {
                input.text("monitor_id", &moving.monitor_id)?;
                input.window(moving.window.as_ref())?;
            }
            Self::CloseWindow(close) => input.window(close.window.as_ref())?,
            Self::ToggleFloating(toggle) => input.window(toggle.window.as_ref())?,
            Self::ToggleFullscreen(toggle) => input.window(toggle.window.as_ref())?,
            Self::Lock(_)
            | Self::Sleep(_)
            | Self::Hibernate(_)
            | Self::Reboot(_)
            | Self::Shutdown(_) => {}
            Self::Screenshot(shot) => {
                if !shot.region_monitor_id.is_empty() {
                    input.text("region_monitor_id", &shot.region_monitor_id)?;
                }
                input.string("output_path", &shot.output_path)?;
                input.require(
                    "output_path",
                    shot.clipboard || !shot.output_path.is_empty(),
                    "a file or clipboard destination is required",
                )?;
            }
            Self::MediaKey(key) => input.require(
                "key",
                matches!(
                    media_key::Key::try_from(key.key),
                    Ok(media_key::Key::MediaPlay
                        | media_key::Key::MediaPause
                        | media_key::Key::MediaPlayPause
                        | media_key::Key::MediaNext
                        | media_key::Key::MediaPrevious
                        | media_key::Key::MediaStop)
                ),
                "must name a media operation",
            )?,
            Self::SetVolume(set) => match set.change {
                Some(set_volume::Change::Absolute(level)) => input.require(
                    "change.absolute",
                    level.is_finite() && (0.0..=1.0).contains(&level),
                    "must be finite and between 0 and 1",
                )?,
                Some(set_volume::Change::Delta(delta)) => {
                    input.require("change.delta", delta.is_finite(), "must be finite")?
                }
                Some(set_volume::Change::ToggleMute(toggle)) => {
                    input.require("change.toggle_mute", toggle, "must be true when selected")?
                }
                None => return input.invalid("change", "is required"),
            },
            Self::SetBacklight(set) => match set.change {
                Some(set_backlight::Change::AbsolutePercent(percent)) => input.require(
                    "change.absolute_percent",
                    percent <= 100,
                    "must be between 0 and 100",
                )?,
                Some(set_backlight::Change::DeltaPercent(_)) => {}
                None => return input.invalid("change", "is required"),
            },
            Self::Notify(notify) => {
                input.string("summary", &notify.summary)?;
                input.string("body", &notify.body)?;
                input.string("icon", &notify.icon)?;
            }
            Self::InvokeUnit(call) => {
                input.require(
                    "unit",
                    UnitName::parse(&call.unit).is_ok(),
                    "must be a unit identifier",
                )?;
                input.require(
                    "command",
                    SurfaceId::parse(&call.command).is_ok(),
                    "must be a command surface identifier",
                )?;
            }
            Self::SetPowerProfile(set) => input.require(
                "profile",
                matches!(
                    PowerProfile::try_from(set.profile),
                    Ok(PowerProfile::Saver | PowerProfile::Balanced | PowerProfile::Performance)
                ),
                "must name a power profile",
            )?,
        }
        Ok(())
    }
}

struct Input(ActionKind);
impl Input {
    fn invalid(&self, field: &'static str, reason: &'static str) -> Result<(), ActionError> {
        Err(ActionError::Invalid {
            action: self.0,
            field,
            reason,
        })
    }
    fn require(
        &self,
        field: &'static str,
        valid: bool,
        reason: &'static str,
    ) -> Result<(), ActionError> {
        if valid {
            Ok(())
        } else {
            self.invalid(field, reason)
        }
    }
    fn string(&self, field: &'static str, value: &str) -> Result<(), ActionError> {
        self.require(field, !value.contains('\0'), "must not contain NUL")
    }
    fn text(&self, field: &'static str, value: &str) -> Result<(), ActionError> {
        self.string(field, value)?;
        self.require(field, !value.trim().is_empty(), "must not be blank")
    }
    fn index(&self, value: u32) -> Result<(), ActionError> {
        self.require("target.index", value > 0, "must be positive")
    }
    fn window(&self, window: Option<&WindowSelector>) -> Result<(), ActionError> {
        match window.and_then(|window| window.target.as_ref()) {
            None => Ok(()),
            Some(window_selector::Target::Focused(focused)) => {
                self.require("window.focused", *focused, "must be true when selected")
            }
            Some(window_selector::Target::AppId(id)) => self.text("window.app_id", id),
            Some(window_selector::Target::Title(title)) => self.text("window.title", title),
        }
    }
}
