//! The supervisor's report on every plugin it runs.

crate::wiring::reading! {
    /// The supervisor's report on every plugin it runs.
    Plugins: omega_proto::omega::PluginsState
}

pub use omega_proto::omega::PluginPhase;

/// What the daemon knows about one plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginReport {
    plugin: String,
    phase: PluginPhase,
    restarts: u32,
    last_exit_code: i32,
    detail: String,
}

impl PluginReport {
    fn of(status: omega_proto::omega::PluginStatus) -> Self {
        Self {
            phase: PluginPhase::try_from(status.phase).unwrap_or(PluginPhase::Unspecified),
            plugin: status.plugin,
            restarts: status.restarts,
            last_exit_code: status.last_exit_code,
            detail: status.detail,
        }
    }

    pub fn plugin(&self) -> &str {
        &self.plugin
    }

    pub fn phase(&self) -> PluginPhase {
        self.phase
    }

    /// Number of process restarts after the initial spawn.
    pub fn restarts(&self) -> u32 {
        self.restarts
    }

    /// Last process exit code, or `None` if terminated by a signal or not yet exited.
    pub fn last_exit_code(&self) -> Option<i32> {
        match self.last_exit_code {
            -1 => None,
            code => Some(code),
        }
    }

    /// Most recent failure description.
    pub fn detail(&self) -> Option<&str> {
        match self.detail.is_empty() {
            true => None,
            false => Some(&self.detail),
        }
    }

    pub fn is_running(&self) -> bool {
        self.phase == PluginPhase::Running
    }

    /// Whether the plugin is failed or waiting to restart after an exit.
    pub fn is_troubled(&self) -> bool {
        matches!(self.phase, PluginPhase::Failed | PluginPhase::Restarting)
    }
}

impl Plugins {
    pub fn all(&self) -> Vec<PluginReport> {
        self.read()
            .map(|state| state.plugins.into_iter().map(PluginReport::of).collect())
            .unwrap_or_default()
    }

    pub fn of(&self, plugin: &str) -> Option<PluginReport> {
        self.all()
            .into_iter()
            .find(|report| report.plugin == plugin)
    }

    /// Return plugins that are failed or waiting to restart.
    pub fn troubled(&self) -> Vec<PluginReport> {
        self.all()
            .into_iter()
            .filter(PluginReport::is_troubled)
            .collect()
    }
}
