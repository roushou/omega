//! Pure action encoding for Hyprland's legacy and Lua configuration providers.

use crate::broker::BrokerError;
use omega_proto::omega::{
    Direction, WindowSelector, action, move_to_workspace, switch_workspace, window_selector,
};

/// Dispatcher syntax selected from the compositor's configuration provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchMode {
    Legacy,
    Lua,
}

impl DispatchMode {
    /// Older compositors without the status query use legacy dispatchers.
    /// Malformed replies and unknown providers are refused before any action.
    pub fn from_status(reply: &str) -> Result<Self, BrokerError> {
        if reply.trim() == "unknown request" {
            return Ok(Self::Legacy);
        }
        #[derive(serde::Deserialize)]
        struct Status {
            #[serde(rename = "configProvider")]
            provider: String,
        }
        let status: Status = serde_json::from_str(reply).map_err(|error| {
            BrokerError::Unreadable(format!("invalid Hyprland status: {error}"))
        })?;
        match status.provider.as_str() {
            "lua" => Ok(Self::Lua),
            "hyprlang" => Ok(Self::Legacy),
            provider => Err(BrokerError::Unreadable(format!(
                "unsupported Hyprland configuration provider: {provider}"
            ))),
        }
    }
}

/// Map a protocol action to a Hyprland dispatcher command.
#[derive(Debug)]
pub struct Dispatch;

impl Dispatch {
    /// Encode an action for the provider reported by this compositor connection.
    pub fn for_mode(action: &action::Kind, mode: DispatchMode) -> Option<String> {
        match mode {
            DispatchMode::Legacy => Self::of(action),
            DispatchMode::Lua => Self::lua(action),
        }
    }

    fn lua(action: &action::Kind) -> Option<String> {
        action.validate().ok()?;
        let command = match action {
            action::Kind::SwitchWorkspace(switch) => format!(
                "hl.dsp.focus({{ workspace = {} }})",
                Self::quoted(&Self::workspace(switch.target.as_ref()?)?)
            ),
            action::Kind::MoveToWorkspace(moving) => format!(
                "hl.dsp.window.move({{ workspace = {}, window = {}, follow = true }})",
                Self::quoted(&Self::destination(moving.target.as_ref()?)?),
                Self::quoted(&Self::window(moving.window.as_ref())?)
            ),
            action::Kind::MoveToMonitor(moving) if Self::is_focused(moving.window.as_ref()) => {
                format!(
                    "hl.dsp.window.move({{ monitor = {}, follow = true }})",
                    Self::quoted(Self::named(&moving.monitor_id)?)
                )
            }
            action::Kind::CloseWindow(close) => format!(
                "hl.dsp.window.close({{ window = {} }})",
                Self::quoted(&Self::window(close.window.as_ref())?)
            ),
            action::Kind::ToggleFloating(toggle) => format!(
                "hl.dsp.window.float({{ window = {}, action = \"toggle\" }})",
                Self::quoted(&Self::window(toggle.window.as_ref())?)
            ),
            action::Kind::ToggleFullscreen(toggle) if Self::is_focused(toggle.window.as_ref()) => {
                "hl.dsp.window.fullscreen({ mode = \"maximized\", action = \"toggle\" })"
                    .to_string()
            }
            _ => return None,
        };
        Some(command)
    }

    // Inputs have no control characters; Lua and JSON share these quote escapes.
    fn quoted(value: &str) -> String {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }

    /// Return the mapped dispatcher or None for unsupported selectors/actions.
    /// Do not guess dispatch strings: unknown dispatchers can receive an ok response.
    pub fn of(action: &action::Kind) -> Option<String> {
        action.validate().ok()?;
        match action {
            action::Kind::SwitchWorkspace(switch) => Some(format!(
                "workspace {}",
                Self::workspace(switch.target.as_ref()?)?
            )),
            action::Kind::MoveToWorkspace(move_to) => Some(format!(
                "movetoworkspace {},{}",
                Self::destination(move_to.target.as_ref()?)?,
                Self::window(move_to.window.as_ref())?
            )),
            action::Kind::MoveToMonitor(move_to) => {
                if !Self::is_focused(move_to.window.as_ref()) {
                    return None;
                }
                Some(format!(
                    "movewindow mon:{}",
                    Self::named(&move_to.monitor_id)?
                ))
            }
            action::Kind::CloseWindow(close) => {
                Some(if Self::is_focused(close.window.as_ref()) {
                    // The focused window has its own dispatcher, and it is the
                    // one that works when nothing matches a selector.
                    "killactive".to_string()
                } else {
                    format!("closewindow {}", Self::window(close.window.as_ref())?)
                })
            }
            action::Kind::ToggleFloating(toggle) => Some(format!(
                "togglefloating {}",
                Self::window(toggle.window.as_ref())?
            )),
            // Fullscreen supports only the focused window; reject explicit other targets.
            action::Kind::ToggleFullscreen(toggle) => {
                Self::is_focused(toggle.window.as_ref()).then(|| "fullscreen 1".to_string())
            }
            _ => None,
        }
    }

    fn workspace(target: &switch_workspace::Target) -> Option<String> {
        match target {
            switch_workspace::Target::Index(index) => Some(index.to_string()),
            switch_workspace::Target::Name(name) => Some(format!("name:{}", Self::named(name)?)),
            switch_workspace::Target::Direction(direction) => match Direction::try_from(*direction)
            {
                Ok(Direction::Next) => Some("e+1".into()),
                Ok(Direction::Previous) => Some("e-1".into()),
                Ok(Direction::Unspecified) | Err(_) => None,
            },
        }
    }

    fn destination(target: &move_to_workspace::Target) -> Option<String> {
        match target {
            move_to_workspace::Target::Index(index) => Some(index.to_string()),
            move_to_workspace::Target::Name(name) => Some(format!("name:{}", Self::named(name)?)),
        }
    }

    /// Convert a window selector; absence selects the focused window.
    fn window(selector: Option<&WindowSelector>) -> Option<String> {
        match selector.and_then(|selector| selector.target.as_ref()) {
            None | Some(window_selector::Target::Focused(_)) => Some("activewindow".to_string()),
            Some(window_selector::Target::AppId(id)) => Some(format!("class:{}", Self::named(id)?)),
            Some(window_selector::Target::Title(title)) => {
                Some(format!("title:{}", Self::named(title)?))
            }
        }
    }

    fn is_focused(selector: Option<&WindowSelector>) -> bool {
        matches!(
            selector.and_then(|selector| selector.target.as_ref()),
            None | Some(window_selector::Target::Focused(_))
        )
    }

    /// Reject empty identifiers and line breaks in dispatcher arguments.
    fn named(name: &str) -> Option<&str> {
        if name.is_empty() || name.chars().any(char::is_control) || name.contains([';', ',']) {
            None
        } else {
            Some(name)
        }
    }
}
