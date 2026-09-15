//! Workspace readings and compositor switching controls.

mod control;
mod reading;

pub use control::WorkspaceControl;
pub use omega_proto::{WorkspaceIndex, WorkspaceIndexError, WorkspaceName, WorkspaceNameError};
pub use reading::{Workspace, Workspaces};
