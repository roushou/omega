//! Broker test drivers matching production connect, read, and action ordering.

// Compiled into every test binary, and each uses a subset: what one test
// does not call is dead there and reachable from nowhere else.
#![allow(dead_code, unreachable_pub)]

use std::time::Duration;

use omega_platform::{Broker, BrokerError};
use omega_proto::omega::StatePatch;

/// Connect and take the initial reading without waiting for a change.
pub(crate) async fn first(broker: &mut impl Broker) -> Result<StatePatch, BrokerError> {
    broker.connect().await?;
    broker.read().await
}

/// Connect before dispatching an action, matching production driver ordering.
pub async fn serve(
    broker: &mut impl Broker,
    action: &omega_proto::omega::action::Kind,
) -> Result<Option<StatePatch>, BrokerError> {
    broker.connect().await?;
    broker.act(action).await
}

/// Check that a subsequent reading waits for a signal or refresh interval.
pub(crate) async fn waits(broker: &mut impl Broker) -> bool {
    tokio::time::timeout(Duration::from_millis(500), broker.wake())
        .await
        .is_err()
}
