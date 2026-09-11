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
    pub(crate) _bytes: tokio::sync::OwnedSemaphorePermit,
    pub answer: oneshot::Sender<Result<result::Outcome, Refusal>>,
}

/// How long a unit gets to answer before the daemon gives up on it. A unit
/// that is wedged must not wedge the reconciler.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("request capacity for {0} exhausted")]
    Full(UnitName),
    #[error("request to {0} exceeds payload limit")]
    TooLarge(UnitName),
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
    link: SessionLink,
}

impl SessionGuard {
    pub(super) fn new(units: super::UnitTable, unit: UnitName, link: SessionLink) -> Self {
        Self { units, unit, link }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.units.disconnected(&self.unit, &self.link);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SessionLink {
    pub(crate) bytes: std::sync::Arc<tokio::sync::Semaphore>,
    pub(crate) requests: tokio::sync::mpsc::Sender<Request>,
    pub(crate) stop: crate::Shutdown,
}

impl SessionGuard {
    pub async fn cancelled(&self) {
        self.link.stop.wait().await;
    }
    pub fn is_current(&self) -> bool {
        !self.link.stop.is_triggered()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::UnitTable;

    #[tokio::test]
    async fn outbound_requests_share_a_byte_budget_across_units() {
        let units = UnitTable::detached(crate::hub::Hub::new());
        let first = UnitName::parse("first").unwrap();
        let second = UnitName::parse("second").unwrap();
        let (a, mut ar) = tokio::sync::mpsc::channel(16);
        let (b, mut br) = tokio::sync::mpsc::channel(16);
        let _ag = units.connected(&first, a);
        let _bg = units.connected(&second, b);
        let op = invoke::Op::SetState(omega_proto::omega::SetState {
            topic: "x".repeat(3 * 1024 * 1024),
            ..Default::default()
        });
        let one = units.request(&first, op.clone());
        let two = units.request(&second, op.clone());
        tokio::pin!(one, two);
        let (a, b) = tokio::select! {
            biased;
            _ = &mut one => panic!("first was not queued"),
            _ = &mut two => panic!("second was not queued"),
            queued = async { (ar.recv().await.unwrap(), br.recv().await.unwrap()) } => queued,
        };
        assert!(matches!(
            units.request(&first, op).await,
            Err(RequestError::Full(_))
        ));
        drop(a);
        drop(b);
        assert_eq!(
            units.inner.request_bytes.available_permits(),
            UnitTable::REQUEST_BYTES
        );
        let oversized = invoke::Op::SetState(omega_proto::omega::SetState {
            topic: "x".repeat(omega_proto::MAX_FRAME_LEN),
            ..Default::default()
        });
        assert!(matches!(
            units.request(&first, oversized).await,
            Err(RequestError::TooLarge(_))
        ));
    }
}
