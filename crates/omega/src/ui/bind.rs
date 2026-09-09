//! What a node does when something happens to it.

use omega_proto::IntoValue;
use omega_proto::omega::{Bind as WireBind, Value};

/// A call back to the unit that drew the node.
///
/// The command is one this unit registered — the daemon refuses any other, so
/// a button is not a way to reach past the unit that drew it. The arguments
/// are what the unit wants handed back with the press:
///
/// ```
/// # use omega::ui::{Bind, Button};
/// # let ssid = "home";
/// Button::new("Connect").on_press(Bind::call("connect").arg(ssid));
/// ```
///
/// Without them a list of forty networks needs forty registered commands,
/// one per row, which is why the plain form exists only for the case that
/// genuinely has nothing to say:
///
/// ```
/// # use omega::ui::Button;
/// Button::new("Toggle").on_press("toggle");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Bind {
    command: String,
    args: Vec<Value>,
}

impl Bind {
    /// Call one of this unit's own commands.
    pub fn call(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
        }
    }

    /// Hand the command another argument. Ordered: a command reads its
    /// arguments by position, and a control that carries a value of its own
    /// appends it after these.
    pub fn arg(mut self, value: impl IntoValue) -> Self {
        self.args.push(value.into_value());
        self
    }

    pub(crate) fn into_wire(self) -> WireBind {
        WireBind {
            command: self.command,
            args: self.args,
        }
    }
}

/// A command with nothing to say to it.
impl From<&str> for Bind {
    fn from(command: &str) -> Self {
        Self::call(command)
    }
}

impl From<String> for Bind {
    fn from(command: String) -> Self {
        Self::call(command)
    }
}
