//! Displays, backlight, compositor workspaces, and input state.

mod backlight;
mod input;
mod monitors;
mod window;
mod workspaces;

pub use backlight::{Backlight, Brightness};
pub use input::Input;
pub use monitors::{Monitor, Monitors};
pub use window::{Focused, Window};
pub use workspaces::{
    Workspace, WorkspaceControl, WorkspaceIndex, WorkspaceIndexError, WorkspaceName,
    WorkspaceNameError, Workspaces,
};
