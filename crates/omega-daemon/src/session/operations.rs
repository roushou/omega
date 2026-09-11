//! Bounded work that must not block a connection's subscription and receive loop.
use super::{Dispatcher, Subscriptions, admission::Peer};
use futures_util::FutureExt;
use omega_proto::{
    Refusal,
    omega::{Frame, Invoke, invoke},
};
use prost::Message;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub(crate) struct Operations {
    tasks: JoinSet<Frame>,
    bytes: Arc<Semaphore>,
}

impl Operations {
    const LIMIT: usize = 16;
    const BYTES: usize = 8 * 1024 * 1024;

    pub(crate) fn new() -> Self {
        Self {
            tasks: JoinSet::new(),
            bytes: Arc::new(Semaphore::new(Self::BYTES)),
        }
    }

    pub(crate) fn deferred(invoke: &Invoke) -> bool {
        matches!(
            invoke.op,
            Some(invoke::Op::Act(_) | invoke::Op::AdoptUnit(_))
        )
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub(crate) fn start(
        &mut self,
        dispatcher: Arc<Dispatcher>,
        peer: Arc<Peer>,
        mut selection: Subscriptions,
        stream_id: u64,
        invoke: Invoke,
    ) -> Result<(), Refusal> {
        if self.tasks.len() >= Self::LIMIT {
            return Err(Refusal::exhausted("too many in-flight operations"));
        }
        let size = invoke.encoded_len();
        if size > omega_proto::MAX_FRAME_LEN {
            return Err(Refusal::too_large("operation exceeds frame limit"));
        }
        let permit = self
            .bytes
            .clone()
            .try_acquire_many_owned(size as u32)
            .map_err(|_| Refusal::exhausted("in-flight operation byte capacity exhausted"))?;
        self.tasks.spawn(async move {
            let _permit = permit;
            let work = async {
                match tokio::time::timeout(
                    crate::units::session::REQUEST_TIMEOUT,
                    dispatcher.invoke(&peer, &mut selection, &invoke),
                )
                .await
                {
                    Ok(Ok(response)) => response.frame(stream_id),
                    Ok(Err(refusal)) => refusal.frame(stream_id),
                    Err(_) => {
                        Refusal::deadline("operation deadline exceeded; completion is unknown")
                            .frame(stream_id)
                    }
                }
            };
            match std::panic::AssertUnwindSafe(work).catch_unwind().await {
                Ok(frame) => frame,
                Err(_) => {
                    tracing::error!(stream_id, peer = %peer.label(), "request task panicked");
                    Refusal::unavailable("request task failed; completion is unknown")
                        .frame(stream_id)
                }
            }
        });
        Ok(())
    }

    pub(crate) async fn next(&mut self) -> Result<Frame, tokio::task::JoinError> {
        self.tasks
            .join_next()
            .await
            .expect("only polled with outstanding operations")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_proto::{
        Manifest, UnitName,
        omega::{Act, Action, RunCommand, action},
    };

    struct Fixture {
        dispatcher: Arc<Dispatcher>,
        peer: Arc<Peer>,
        selection: Subscriptions,
    }
    impl Fixture {
        fn new() -> Self {
            let hub = crate::hub::Hub::new();
            let units = crate::units::UnitTable::detached(hub.clone());
            let shutdown = crate::Shutdown::new();
            let supervisor = crate::supervisor::Supervisor::new(
                omega_proto::Socket::at("/unused"),
                units.clone(),
                shutdown.clone(),
            );
            let brokers = crate::broker::Brokerage::new(hub.clone(), shutdown);
            let name = UnitName::parse("example").unwrap();
            let manifest = Manifest::new(&name, "1");
            Self {
                dispatcher: Arc::new(Dispatcher::new(hub, supervisor, units, brokers)),
                peer: Arc::new(Peer::unit(name.clone(), &manifest).unwrap()),
                selection: Subscriptions::of(&name, &manifest),
            }
        }
        fn start(
            &self,
            operations: &mut Operations,
            stream: u64,
            bytes: usize,
        ) -> Result<(), Refusal> {
            operations.start(
                self.dispatcher.clone(),
                self.peer.clone(),
                self.selection.clone(),
                stream,
                Invoke {
                    op: Some(invoke::Op::Act(Act {
                        action: Some(Action {
                            kind: Some(action::Kind::RunCommand(RunCommand {
                                command: "x".repeat(bytes),
                            })),
                        }),
                    })),
                },
            )
        }
    }

    #[tokio::test]
    async fn operation_count_is_bounded_and_refusals_keep_their_streams() {
        let fixture = Fixture::new();
        let mut operations = Operations::new();
        for stream in 0..Operations::LIMIT {
            fixture.start(&mut operations, stream as u64, 1).unwrap();
        }
        assert_eq!(
            fixture.start(&mut operations, 99, 1).unwrap_err().code,
            omega_proto::omega::ErrorCode::ResourceExhausted
        );
        let mut streams = std::collections::BTreeSet::new();
        while !operations.is_empty() {
            let answer = operations.next().await.unwrap();
            assert_eq!(
                Refusal::of(&answer).unwrap().code,
                omega_proto::omega::ErrorCode::PermissionDenied
            );
            streams.insert(answer.stream_id);
        }
        assert_eq!(streams.len(), Operations::LIMIT);
        assert_eq!(operations.bytes.available_permits(), Operations::BYTES);
    }

    #[tokio::test]
    async fn operation_bytes_are_bounded_before_dispatch() {
        let fixture = Fixture::new();
        let mut operations = Operations::new();
        fixture.start(&mut operations, 1, 3_000_000).unwrap();
        fixture.start(&mut operations, 3, 3_000_000).unwrap();
        assert_eq!(
            fixture
                .start(&mut operations, 5, 3_000_000)
                .unwrap_err()
                .code,
            omega_proto::omega::ErrorCode::ResourceExhausted
        );
    }
}
