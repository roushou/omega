//! Session activity and lock state.

crate::wiring::reading! {
    /// Session activity and lock state.
    Idle: omega_proto::omega::IdleState
}

use std::time::{Duration, SystemTime, UNIX_EPOCH};

impl Idle {
    /// Whether the session has gone idle.
    pub fn is_idle(&self) -> bool {
        self.read().is_some_and(|idle| idle.idle)
    }

    /// Whether the session is locked. Independent of idle state.
    pub fn is_locked(&self) -> bool {
        self.read().is_some_and(|idle| idle.locked)
    }

    /// Time the session became idle, or `None` if no idle timestamp is available.
    pub fn since(&self) -> Option<SystemTime> {
        let idle = self.read()?;
        match idle.idle_since {
            0 => None,
            seconds => Some(UNIX_EPOCH + Duration::from_secs(seconds)),
        }
    }

    /// Current idle duration, or `None` if no idle timestamp is available.
    pub fn how_long(&self) -> Option<Duration> {
        SystemTime::now().duration_since(self.since()?).ok()
    }
}
