//! Closed action vocabulary and required capabilities.
//! The exhaustive wire mapping requires every new action to declare its permission.
//! Authorization precedes handler availability checks.

use crate::omega::{Capability, action};

/// `None` is a stated cost, not an omitted one.
macro_rules! cost {
    (None) => {
        None
    };
    ($capability:ident) => {
        Some(Capability::$capability)
    };
}

/// Generate action kinds, names, capabilities, and exhaustive wire conversion.
macro_rules! actions {
    ($(
        $(#[$meta:meta])*
        $variant:ident => $cost:ident,
    )*) => {
        /// What an action is, without its arguments.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum ActionKind {
            $($(#[$meta])* $variant,)*
        }

        impl ActionKind {
            /// Every kind, in declaration order. Generated from the same row
            /// as the variant, so it cannot omit one.
            pub const ALL: &'static [ActionKind] = &[$(Self::$variant,)*];

            /// The kind of a wire action. Exhaustive: a new arm in
            /// `action.proto` does not compile until it has a row here.
            pub fn of(action: &action::Kind) -> Self {
                match action {
                    $(action::Kind::$variant(_) => Self::$variant,)*
                }
            }

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => stringify!($variant),)*
                }
            }

            /// Additional required capability, or `None` when no action capability is needed.
            /// Scope and handler checks still apply.
            pub fn cost(self) -> Option<Capability> {
                match self {
                    $(Self::$variant => cost!($cost),)*
                }
            }
        }
    };
}

actions! {
    LaunchApp => Spawn,
    RunCommand => Spawn,
    CaptureCommand => Spawn,
    SwitchWorkspace => None,
    MoveToWorkspace => None,
    MoveToMonitor => None,
    CloseWindow => None,
    Lock => SystemControl,
    Sleep => SystemControl,
    Hibernate => SystemControl,
    Reboot => SystemControl,
    Shutdown => SystemControl,
    Screenshot => Screenshot,
    MediaKey => Media,
    SetVolume => Audio,
    SetBacklight => Backlight,
    Notify => Notify,
    /// Exact command dependencies are authorized by the command dispatcher.
    InvokePlugin => None,
    ToggleFloating => None,
    ToggleFullscreen => None,
    SetPowerProfile => SystemControl,
    ConnectWifi => Network,
    DisconnectWifi => Network,
    ConnectBluetooth => Bluetooth,
    DisconnectBluetooth => Bluetooth,
    CaptureText => Screenshot,
    RecordScreen => Screenshot,
    WriteClipboard => Clipboard,
    ClearClipboard => Clipboard,
    SetStreamVolume => Audio,
    SetStreamMute => Audio,
    SetDefaultSink => Audio,
    SetInputMute => Audio,
    SetInputVolume => Audio,
}

mod validation;
pub use validation::ActionError;

#[cfg(test)]
mod tests;
