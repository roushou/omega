//! Compositor workspaces and active workspace selection.

crate::wiring::reading! {
    /// Compositor workspaces and active workspace selection.
    Workspaces: omega_proto::omega::WorkspacesState
}

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

    /// Compositor workspace ID. Stable across renames; suitable for item keys.
    pub fn id(&self) -> i32 {
        self.id
    }

    /// Workspace display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Output identifier, such as `eDP-1`.
    pub fn monitor(&self) -> &str {
        &self.monitor
    }

    /// Number of windows on this workspace.
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

    /// Return the active workspace, if available.
    pub fn active(&self) -> Option<Workspace> {
        self.all().into_iter().find(Workspace::is_active)
    }

    /// Return workspaces assigned to the given output identifier.
    pub fn on(&self, monitor: &str) -> Vec<Workspace> {
        self.all()
            .into_iter()
            .filter(|workspace| workspace.monitor == monitor)
            .collect()
    }
}
