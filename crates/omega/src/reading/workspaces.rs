//! The workspaces, and which is being looked at.

use crate::reading::Workspaces;

/// One workspace, as the compositor reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    id: i32,
    name: String,
    monitor: String,
    windows: u32,
    active: bool,
}

impl Workspace {
    fn of(workspace: omega_proto::omega::WorkspaceInfo) -> Self {
        Self {
            id: workspace.id,
            name: workspace.name,
            monitor: workspace.monitor_id,
            windows: workspace.windows,
            active: workspace.active,
        }
    }

    /// The compositor's own id, which survives a rename — so a list keys
    /// rows by this rather than by what it is called.
    pub fn id(&self) -> i32 {
        self.id
    }

    /// `1`, or whatever it was named.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `eDP-1`.
    pub fn monitor(&self) -> &str {
        &self.monitor
    }

    /// How many windows are on it. Nought is an empty workspace, which a bar
    /// usually draws quieter rather than not at all.
    pub fn windows(&self) -> u32 {
        self.windows
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn is_empty(&self) -> bool {
        self.windows == 0
    }
}

impl Workspaces {
    pub fn all(&self) -> Vec<Workspace> {
        self.read()
            .map(|state| state.workspaces.into_iter().map(Workspace::of).collect())
            .unwrap_or_default()
    }

    /// The one being looked at.
    pub fn active(&self) -> Option<Workspace> {
        self.all().into_iter().find(Workspace::is_active)
    }

    /// The ones on a given monitor, for a bar pinned to one.
    pub fn on(&self, monitor: &str) -> Vec<Workspace> {
        self.all()
            .into_iter()
            .filter(|workspace| workspace.monitor == monitor)
            .collect()
    }
}
