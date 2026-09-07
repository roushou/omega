//! Asking a unit for something.
//!
//! The protocol runs both ways: the daemon invokes a unit's surfaces, not
//! only the other way round. A session registers itself here for as long as
//! it lasts, so the rest of the daemon can reach a unit by name without
//! knowing anything about connections.
//!
//! Stream ids are allocated from both ends of one connection, so they are
//! split by parity: the daemon's requests are even, a unit's are odd. A
//! `Result` is answered by whoever allocated the stream it arrives on.

use std::time::Duration;

use tokio::sync::oneshot;

use omega_proto::Refusal;
use omega_proto::UnitName;
use omega_proto::omega::{invoke, result};

/// One request to a unit, and where its answer goes.
#[derive(Debug)]
pub struct Request {
    pub op: invoke::Op,
    pub answer: oneshot::Sender<Result<result::Outcome, Refusal>>,
}

/// How long a unit gets to answer before the daemon gives up on it. A unit
/// that is wedged must not wedge the reconciler.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("{0} is not connected")]
    Absent(UnitName),
    #[error("{0} did not answer in time")]
    Timeout(UnitName),
    #[error("{unit} refused: {source}")]
    Refused {
        unit: UnitName,
        #[source]
        source: Refusal,
    },
}

/// Deregisters a unit when its session ends, however it ends.
#[derive(Debug)]
pub struct SessionGuard {
    units: super::UnitTable,
    unit: UnitName,
}

impl SessionGuard {
    pub(super) fn new(units: super::UnitTable, unit: UnitName) -> Self {
        Self { units, unit }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.units.disconnected(&self.unit);
    }
}
