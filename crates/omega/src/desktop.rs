//! Displays, brightness, workspaces, focused windows, and keyboard state.
//!
//! Holding a state handle subscribes the plugin to its topics. Control handles
//! belong on commands or reactions; a widget cannot hold them.
//!
//! ```
//! use omega::desktop::Window;
//!
//! #[derive(omega::Widget)]
//! struct Indicator {
//!     window: Window,
//! }
//! ```

pub use crate::effect::brightness::Brightness;
pub use crate::reading::{
    Backlight, Focused, Input, Monitor, Monitors, Window, Workspace, Workspaces,
};
