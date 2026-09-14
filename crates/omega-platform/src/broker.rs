//! What a broker is.

use std::time::Duration;

use async_trait::async_trait;
use tokio::time::{Interval, MissedTickBehavior};

use omega_proto::omega::{StatePatch, action};
use omega_proto::{ActionKind, SystemTopic};

/// Subsystem connection, reading, and action interface.
/// Declare supported topics and actions independently of connection state.
/// The daemon driver owns retries, shutdown, and initial reads; implementations
/// own connection resources and subsystem conversion.
#[async_trait]
pub trait Broker: Send + 'static {
    /// For logs and `omega status`. Names the subsystem, not the topic —
    /// `"upower"`, not `"battery"`.
    fn name(&self) -> &'static str;

    /// The topics this broker projects. One connection often serves several:
    /// UPower reports the battery *and* every peripheral's.
    fn topics(&self) -> &'static [SystemTopic];

    /// The action kinds this broker serves. Empty for a broker that only
    /// reports.
    fn actions(&self) -> &'static [ActionKind] {
        &[]
    }

    /// Open the subsystem connection before reads and after connection failures.
    /// The default is a no-op for connectionless brokers.
    async fn connect(&mut self) -> Result<(), BrokerError> {
        Ok(())
    }

    /// Release connection state after a failed or cancelled operation.
    /// Connection-owning brokers must drop every link here. This operation
    /// must not block; reconnect always starts from a disconnected state.
    fn disconnect(&mut self) {}

    /// Wait for a signal or polling interval. The default remains pending.
    /// Must be cancellation-safe: retain partial input in the broker, not the future.
    /// An error closes the connection and schedules reconnection.
    async fn wake(&mut self) -> Result<(), BrokerError> {
        std::future::pending().await
    }

    /// Read after connection and after each successful wake.
    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        Ok(StatePatch::default())
    }

    /// Execute a declared action and return any resulting state patch.
    /// Return `None` when no observable state changed.
    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        Err(BrokerError::Unserved(ActionKind::of(action)))
    }
}

/// Lazy Tokio polling interval with missed ticks skipped.
/// Construction must not require an active runtime.
#[derive(Debug)]
pub(crate) struct Cadence {
    every: Duration,
    /// Whether the first turn comes at once or after a period.
    immediate: bool,
    tick: Option<Interval>,
}

impl Cadence {
    /// Create a polling interval with an immediate first tick.
    pub(crate) fn every(every: Duration) -> Self {
        Self {
            every,
            immediate: true,
            tick: None,
        }
    }

    /// Create a fallback interval whose first tick occurs after one period.
    pub(crate) fn after(every: Duration) -> Self {
        Self {
            every,
            immediate: false,
            tick: None,
        }
    }

    /// Wait for the next turn. Cancel-safe.
    pub(crate) async fn wait(&mut self) {
        let every = self.every;
        let immediate = self.immediate;
        self.tick
            .get_or_insert_with(|| {
                let start = if immediate {
                    tokio::time::Instant::now()
                } else {
                    tokio::time::Instant::now() + every
                };
                let mut tick = tokio::time::interval_at(start, every);
                tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
                tick
            })
            .tick()
            .await;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    #[error("{0}")]
    Unsupported(String),
    #[error("broker operation deadline elapsed; execution may have started")]
    Timeout,
    #[error("broker action capacity exhausted")]
    Full,
    #[error("broker action exceeds payload limit")]
    TooLarge,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// Subsystem connection, parsing, or operation failure.
    /// The driver adds broker identity when reporting errors.
    #[error("{0}")]
    Unreadable(String),
    /// Routed here by mistake: the kind is not one this broker declared.
    #[error("{} is not served by this broker", .0.name())]
    Unserved(ActionKind),
}

impl BrokerError {
    /// Underlying subsystem client error.
    pub fn unreadable(error: impl std::fmt::Display) -> Self {
        Self::Unreadable(error.to_string())
    }

    /// The connection is not there — closed under us, or never opened. The
    /// driver reopens before the next reading.
    pub fn gone() -> Self {
        Self::Unreadable("the connection is gone".into())
    }
}

/// Implement name-only Debug for brokers with non-Debug connection fields.
macro_rules! opaque_debug {
    ($type:ident) => {
        impl std::fmt::Debug for $type {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(stringify!($type))
            }
        }
    };
}

pub(crate) use opaque_debug;
