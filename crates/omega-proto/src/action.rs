//! Actions: the closed taxonomy of what a unit may make the machine do.
//!
//! One table. [`ActionKind`] names every action in `action.proto` and the
//! capability each one costs, and the match that reads a wire action is
//! exhaustive — so the schema growing is a compile error here until the new
//! action's cost is stated.
//!
//! Authorization is complete even where implementation is not: an action the
//! daemon cannot perform yet is refused for the right reason first, so a unit
//! is never granted something by the accident of a missing handler.
//!
//! This lives in the protocol crate rather than the daemon because a broker
//! declares the kinds it serves, and a broker must not depend on the daemon.

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

/// Declare the action kinds: the enum, `ALL`, the name, the capability each
/// costs, and the mapping from a wire action.
///
/// The variant is also the `action::Kind` variant, so a row naming an action
/// `action.proto` does not have will not compile.
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

            /// The capability this action costs. `None` is not "free": it
            /// means the action affects only the unit's own surfaces, and the
            /// daemon still has to be able to perform it.
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
    SetSetting => SystemControl,
    ToggleSetting => SystemControl,
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
    /// Making another unit run its own code is making code run.
    InvokeUnit => Spawn,
    ToggleFloating => None,
    ToggleFullscreen => None,
}
