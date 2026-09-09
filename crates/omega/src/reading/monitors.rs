//! The monitors the compositor is driving.

use crate::reading::Monitors;

/// One monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    id: String,
    connected: bool,
    width: u32,
    height: u32,
    refresh_mhz: u32,
    x: i32,
    y: i32,
    scale: f32,
    primary: bool,
}

impl Monitor {
    fn of(monitor: omega_proto::omega::MonitorInfo) -> Self {
        Self {
            id: monitor.id,
            connected: monitor.connected,
            width: monitor.width,
            height: monitor.height,
            refresh_mhz: monitor.refresh_mhz,
            x: monitor.x,
            y: monitor.y,
            scale: monitor.scale,
            primary: monitor.primary,
        }
    }

    /// `eDP-1`.
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Refresh rate in hertz. The wire carries millihertz, because 59.951 Hz
    /// is a real mode and rounding it to 60 loses which mode it is.
    pub fn refresh_hz(&self) -> f64 {
        f64::from(self.refresh_mhz) / 1000.0
    }

    /// Where its top-left corner sits in the compositor's layout.
    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn is_primary(&self) -> bool {
        self.primary
    }
}

impl Monitors {
    pub fn all(&self) -> Vec<Monitor> {
        self.read()
            .map(|state| state.monitors.into_iter().map(Monitor::of).collect())
            .unwrap_or_default()
    }

    pub fn primary(&self) -> Option<Monitor> {
        self.all().into_iter().find(Monitor::is_primary)
    }

    pub fn at(&self, id: &str) -> Option<Monitor> {
        self.all().into_iter().find(|monitor| monitor.id == id)
    }
}
