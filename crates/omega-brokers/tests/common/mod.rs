//! Driving a broker the way the daemon drives one.
//!
//! A broker says how to connect, wake and read; the daemon's driver says
//! when. A test that called those in its own order would be testing a
//! sequence nothing runs — so these two are the sequence, and every live test
//! goes through them.

// Compiled into every test binary, and each uses a subset: what one test
// does not call is dead there and reachable from nowhere else.
#![allow(dead_code, unreachable_pub)]

use std::time::Duration;

use omega_brokers::{Broker, BrokerError};
use omega_proto::omega::StatePatch;

/// Connect, and take the first reading.
///
/// At once, without waiting: a broker's first reading is what the driver takes
/// the moment it has a connection, or a bar is blank until something moves.
pub(crate) async fn first(broker: &mut impl Broker) -> Result<StatePatch, BrokerError> {
    broker.connect().await?;
    broker.read().await
}

/// Serve one action, the way the driver serves one.
///
/// Connecting first, because the driver has always connected by the time an
/// action reaches a broker — a test that skipped it would be asking a broker
/// to act through a connection it was never given.
pub async fn serve(
    broker: &mut impl Broker,
    action: &omega_proto::omega::action::Kind,
) -> Result<Option<StatePatch>, BrokerError> {
    broker.connect().await?;
    broker.act(action).await
}

/// Whether a second reading would have to wait.
///
/// The one property every reporting broker owes: after the first reading it
/// waits for something to happen. A broker that answered again at once would
/// be a hot loop wearing a signal stream, and the driver would spin on it.
pub(crate) async fn waits(broker: &mut impl Broker) -> bool {
    tokio::time::timeout(Duration::from_millis(500), broker.wake())
        .await
        .is_err()
}
