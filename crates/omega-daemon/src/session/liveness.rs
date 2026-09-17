//! Session keepalives and silence deadlines using Tokio monotonic time.

use std::time::Duration;

use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Alive,
    /// Nothing heard within the timeout; the session should end.
    Unresponsive,
}

#[derive(Debug, Clone)]
pub struct Liveness {
    interval: Duration,
    timeout: Duration,
    last_seen: Instant,
}

impl Liveness {
    /// Ping every 15s, give up after 45s — three missed pings, which
    /// tolerates a plugin briefly busy without holding a dead one for long.
    pub const INTERVAL: Duration = Duration::from_secs(15);
    pub const TIMEOUT: Duration = Duration::from_secs(45);

    pub fn new() -> Self {
        Self::with(Self::INTERVAL, Self::TIMEOUT)
    }

    pub fn with(interval: Duration, timeout: Duration) -> Self {
        Self {
            interval,
            timeout,
            last_seen: Instant::now(),
        }
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Any frame from the peer proves it is alive — a `Pong` is only the
    /// answer of last resort, for a plugin with nothing to say.
    pub fn seen(&mut self) {
        self.last_seen = Instant::now();
    }

    pub fn health_at(&self, now: Instant) -> Health {
        if now.duration_since(self.last_seen) > self.timeout {
            Health::Unresponsive
        } else {
            Health::Alive
        }
    }

    pub fn health(&self) -> Health {
        self.health_at(Instant::now())
    }
}

impl Default for Liveness {
    fn default() -> Self {
        Self::new()
    }
}
