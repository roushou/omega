//! Focused window state.

crate::wiring::reading! {
    /// Focused window state.
    Window: omega_proto::omega::WindowState
}

/// The focused window.
#[derive(Debug, Clone, PartialEq)]
pub struct Focused {
    app_id: String,
    title: String,
    workspace: String,
    monitor: String,
    pid: u32,
    floating: bool,
    fullscreen: bool,
}

impl Focused {
    fn of(window: omega_proto::omega::WindowInfo) -> Self {
        Self {
            app_id: window.app_id,
            title: window.title,
            workspace: window.workspace,
            monitor: window.monitor_id,
            pid: window.pid,
            floating: window.floating,
            fullscreen: window.fullscreen,
        }
    }

    /// Application identifier reported by the compositor.
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// The workspace's name, not its id.
    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    pub fn monitor(&self) -> &str {
        &self.monitor
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn is_floating(&self) -> bool {
        self.floating
    }

    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }
}

impl Window {
    /// Return the focused window, or `None` if no window has focus.
    pub fn focused(&self) -> Option<Focused> {
        self.read()?.focused.map(Focused::of)
    }

    /// Focused window title, or `None` if no window has focus.
    pub fn title(&self) -> Option<String> {
        self.focused()
            .map(|window| window.title)
            .filter(|title| !title.is_empty())
    }
}
