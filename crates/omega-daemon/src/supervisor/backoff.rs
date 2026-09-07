//! Restart pacing.

use std::time::Duration;

/// Exponential backoff with jitter, capped.
///
/// A unit that fails on startup would otherwise respawn as fast as the kernel
/// can fork; a unit that failed once an hour ago should not be punished for
/// it, so a run that lasted is treated as a success and resets the delay.
#[derive(Debug, Clone)]
pub struct Backoff {
    attempt: u32,
    base: Duration,
    max: Duration,
}

impl Backoff {
    pub const BASE: Duration = Duration::from_millis(500);
    pub const MAX: Duration = Duration::from_secs(30);
    /// A run at least this long counts as healthy.
    pub const HEALTHY: Duration = Duration::from_secs(10);

    pub fn new() -> Self {
        Self::with(Self::BASE, Self::MAX)
    }

    pub fn with(base: Duration, max: Duration) -> Self {
        Self {
            attempt: 0,
            base,
            max,
        }
    }

    /// The delay before the next spawn.
    pub fn delay(&mut self) -> Duration {
        let exponential = self.base.saturating_mul(1 << self.attempt.min(16));
        self.attempt = self.attempt.saturating_add(1);
        Self::jitter(exponential.min(self.max))
    }

    /// Forget the failures: this unit ran long enough to count.
    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    pub fn attempts(&self) -> u32 {
        self.attempt
    }

    /// Spread restarts so a machine waking up does not respawn every unit on
    /// the same tick. Deterministic in width, random in choice.
    fn jitter(delay: Duration) -> Duration {
        let mut byte = [0u8; 1];
        if getrandom::fill(&mut byte).is_err() {
            return delay;
        }
        // ±12.5%, which is enough to decorrelate without changing the shape.
        let spread = (delay.as_millis() as u64 / 8).max(1);
        let offset = (byte[0] as u64 % spread) as i64 - (spread / 2) as i64;
        Duration::from_millis((delay.as_millis() as i64 + offset).max(0) as u64)
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}
