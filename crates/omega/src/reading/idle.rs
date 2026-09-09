//! Whether anybody is using the machine.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::reading::Idle;

impl Idle {
    /// Whether the session has gone idle.
    pub fn is_idle(&self) -> bool {
        self.read().is_some_and(|idle| idle.idle)
    }

    /// Whether it is locked. Not the same question: a locked session is not
    /// necessarily idle and an idle one is not necessarily locked.
    pub fn is_locked(&self) -> bool {
        self.read().is_some_and(|idle| idle.locked)
    }

    /// When it went idle. `None` while somebody is using it, which is what
    /// the wire's nought means.
    pub fn since(&self) -> Option<SystemTime> {
        let idle = self.read()?;
        match idle.idle_since {
            0 => None,
            seconds => Some(UNIX_EPOCH + Duration::from_secs(seconds)),
        }
    }

    /// How long it has been idle, or `None` if it is not.
    pub fn how_long(&self) -> Option<Duration> {
        SystemTime::now().duration_since(self.since()?).ok()
    }
}
