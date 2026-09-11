use super::{Completion, EffectError, Receipt, Submission};
use omega_proto::omega::{invoke, result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::time::Instant;

/// Capacity covers queued requests and requests still outstanding on the wire.
#[derive(Clone, Debug)]
pub(crate) struct EffectsSender {
    sender: mpsc::Sender<Request>,
    capacity: Arc<Semaphore>,
    bytes: Arc<Semaphore>,
}
impl EffectsSender {
    pub(crate) fn submit(&self, op: invoke::Op) -> Submission {
        let permit = self.reserve(op.encoded_len())?;
        permit.submit(op)
    }
    pub(crate) fn reserve_record(&self) -> Result<Admission, EffectError> {
        self.reserve(Effects::MAX_PAYLOAD)
    }
    fn reserve(&self, size: usize) -> Result<Admission, EffectError> {
        if size > Effects::MAX_PAYLOAD {
            return Err(EffectError::TooLarge);
        }
        let bytes = self
            .bytes
            .clone()
            .try_acquire_many_owned(size as u32)
            .map_err(|error| match error {
                tokio::sync::TryAcquireError::Closed => EffectError::Closed,
                tokio::sync::TryAcquireError::NoPermits => EffectError::Full,
            })?;
        let permit = self
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|error| match error {
                tokio::sync::TryAcquireError::Closed => EffectError::Closed,
                tokio::sync::TryAcquireError::NoPermits => EffectError::Full,
            })?;
        let slot = self
            .sender
            .clone()
            .try_reserve_owned()
            .map_err(|error| match error {
                mpsc::error::TrySendError::Closed(_) => EffectError::Closed,
                mpsc::error::TrySendError::Full(_) => EffectError::Full,
            })?;
        Ok(Admission {
            slot,
            permit,
            bytes,
        })
    }
}

pub(crate) struct Admission {
    slot: mpsc::OwnedPermit<Request>,
    permit: OwnedSemaphorePermit,
    bytes: OwnedSemaphorePermit,
}
impl Admission {
    pub(crate) fn submit(mut self, op: invoke::Op) -> Submission {
        let size = op.encoded_len();
        if size > self.bytes.num_permits() {
            return Err(EffectError::TooLarge);
        }
        drop(self.bytes.split(self.bytes.num_permits() - size));
        let (reply, receiver) = oneshot::channel();
        self.slot.send(Request {
            op,
            pending: PendingEffect {
                reply: Some(reply),
                _permit: self.permit,
                _bytes: self.bytes,
                deadline: Instant::now() + Effects::TIMEOUT,
            },
        });
        Ok(Receipt { receiver })
    }
}

pub(crate) struct Request {
    op: invoke::Op,
    pending: PendingEffect,
}
impl Request {
    pub(crate) fn complete(mut self, result: Completion) -> Result<invoke::Op, EffectError> {
        self.pending.complete(result)?;
        Ok(self.op)
    }
}

struct PendingEffect {
    reply: Option<oneshot::Sender<Completion>>,
    _permit: OwnedSemaphorePermit,
    _bytes: OwnedSemaphorePermit,
    deadline: Instant,
}
impl PendingEffect {
    fn complete(&mut self, result: Completion) -> Result<(), EffectError> {
        if let Some(reply) = self.reply.take()
            && let Err(result) = reply.send(result)
        {
            result?;
        }
        Ok(())
    }
}

pub(crate) struct Effects {
    receiver: mpsc::Receiver<Request>,
    capacity: Arc<Semaphore>,
    bytes: Arc<Semaphore>,
    pending: HashMap<u64, PendingEffect>,
}
impl Effects {
    pub(crate) const LIMIT: usize = 64;
    const BYTE_LIMIT: usize = 8 * 1024 * 1024;
    // Leave room for the invoke/frame envelope and a maximum-width stream id.
    const MAX_PAYLOAD: usize = omega_proto::MAX_FRAME_LEN - 32;
    pub(crate) const TIMEOUT: Duration = Duration::from_secs(5);
    pub(crate) fn channel() -> (EffectsSender, Self) {
        let (sender, receiver) = mpsc::channel(Self::LIMIT);
        let capacity = Arc::new(Semaphore::new(Self::LIMIT));
        let bytes = Arc::new(Semaphore::new(Self::BYTE_LIMIT));
        (
            EffectsSender {
                sender,
                capacity: capacity.clone(),
                bytes: bytes.clone(),
            },
            Self {
                receiver,
                capacity,
                bytes,
                pending: HashMap::new(),
            },
        )
    }
    pub(crate) async fn recv(&mut self) -> Option<Request> {
        self.receiver.recv().await
    }
    pub(crate) fn try_recv(&mut self) -> Option<Request> {
        self.receiver.try_recv().ok()
    }
    pub(crate) fn begin(
        &mut self,
        stream: u64,
        request: Request,
    ) -> Result<Option<invoke::Op>, EffectError> {
        let Request { op, mut pending } = request;
        if pending.deadline <= Instant::now() {
            pending.complete(Err(EffectError::Timeout))?;
            return Ok(None);
        }
        self.pending.insert(stream, pending);
        Ok(Some(op))
    }
    pub(crate) fn answer(
        &mut self,
        stream: u64,
        answer: &omega_proto::omega::Result,
    ) -> Result<bool, EffectError> {
        let Some(mut request) = self.pending.remove(&stream) else {
            return Ok(false);
        };
        if request.deadline <= Instant::now() {
            request.complete(Err(EffectError::Timeout))?;
        }
        if !answer.done {
            self.pending.insert(stream, request);
            return Ok(true);
        }
        let result = match &answer.outcome {
            Some(result::Outcome::Ok(_)) => Ok(None),
            Some(result::Outcome::Value(value)) => Ok(Some(value.clone())),
            Some(result::Outcome::Error(error)) => {
                Err(EffectError::Refused(omega_proto::Refusal::new(
                    omega_proto::omega::ErrorCode::try_from(error.code).unwrap_or_default(),
                    error.message.clone(),
                )))
            }
            _ => Err(EffectError::UnexpectedResponse),
        };
        request.complete(result)?;
        Ok(true)
    }
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.pending
            .values()
            .filter(|request| request.reply.is_some())
            .map(|request| request.deadline)
            .min()
    }
    pub(crate) fn expire(&mut self) -> Result<(), EffectError> {
        for request in self.pending.values_mut() {
            if request.deadline <= Instant::now() {
                request.complete(Err(EffectError::Timeout))?;
            }
        }
        // Keep timed-out wire requests leased until a terminal answer: a timeout
        // does not establish that the daemon stopped executing them.
        Ok(())
    }
}
impl Drop for Effects {
    fn drop(&mut self) {
        self.capacity.close();
        self.bytes.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_proto::omega::{ErrorCode, Result as OpResult};

    struct Fixture;
    impl Fixture {
        fn op() -> invoke::Op {
            invoke::Op::SetState(Default::default())
        }
        fn success() -> OpResult {
            OpResult {
                done: true,
                outcome: Some(result::Outcome::Ok(Default::default())),
            }
        }
    }

    #[tokio::test]
    async fn intermediate_results_keep_the_receipt_pending_and_capacity_held() {
        let (sender, mut effects) = Effects::channel();
        let receipt = sender.submit(Fixture::op()).unwrap();
        let request = effects.recv().await.unwrap();
        effects.begin(1, request).unwrap();
        let bytes = sender.bytes.available_permits();
        let mut intermediate = Fixture::success();
        intermediate.done = false;
        assert!(effects.answer(1, &intermediate).unwrap());
        let waiting = receipt.wait();
        tokio::pin!(waiting);
        tokio::select! {
            biased;
            result = &mut waiting => panic!("completed on a nonterminal result: {result:?}"),
            _ = tokio::task::yield_now() => {}
        }
        assert_eq!(sender.capacity.available_permits(), Effects::LIMIT - 1);
        assert_eq!(sender.bytes.available_permits(), bytes);
        effects.answer(1, &Fixture::success()).unwrap();
        waiting.await.unwrap();
        assert_eq!(sender.capacity.available_permits(), Effects::LIMIT);
        assert_eq!(sender.bytes.available_permits(), Effects::BYTE_LIMIT);
    }

    #[tokio::test]
    async fn sending_moves_the_payload_but_keeps_admission_until_completion() {
        let (sender, mut effects) = Effects::channel();
        let topic = "x".repeat(1024);
        let pointer = topic.as_ptr();
        let receipt = sender
            .submit(invoke::Op::SetState(omega_proto::omega::SetState {
                topic,
                ..Default::default()
            }))
            .unwrap();
        let request = effects.recv().await.unwrap();
        let op = effects.begin(1, request).unwrap().unwrap();
        let invoke::Op::SetState(state) = op else {
            panic!("wrong operation")
        };
        assert_eq!(state.topic.as_ptr(), pointer);
        drop(state);
        assert_eq!(sender.capacity.available_permits(), Effects::LIMIT - 1);
        effects.answer(1, &Fixture::success()).unwrap();
        receipt.wait().await.unwrap();
        assert_eq!(sender.capacity.available_permits(), Effects::LIMIT);
        assert_eq!(sender.bytes.available_permits(), Effects::BYTE_LIMIT);
    }

    #[tokio::test]
    async fn byte_capacity_covers_queued_and_sent_payloads_and_releases_on_answer() {
        let (sender, mut effects) = Effects::channel();
        let op = invoke::Op::SetState(omega_proto::omega::SetState {
            topic: "x".repeat(3 * 1024 * 1024),
            ..Default::default()
        });
        let first = sender.submit(op.clone()).unwrap();
        let second = sender.submit(op.clone()).unwrap();
        assert!(matches!(sender.submit(op.clone()), Err(EffectError::Full)));
        let request = effects.recv().await.unwrap();
        effects.begin(1, request).unwrap();
        assert!(matches!(sender.submit(op.clone()), Err(EffectError::Full)));
        effects.answer(1, &Fixture::success()).unwrap();
        first.wait().await.unwrap();
        sender.submit(op).unwrap().detach();
        drop(effects);
        assert!(matches!(second.wait().await, Err(EffectError::Closed)));
        assert_eq!(sender.bytes.available_permits(), Effects::BYTE_LIMIT);
    }

    #[tokio::test]
    async fn oversized_payloads_and_failed_reservations_leave_capacity_unchanged() {
        let (sender, _effects) = Effects::channel();
        let op = invoke::Op::SetState(omega_proto::omega::SetState {
            topic: "x".repeat(Effects::MAX_PAYLOAD),
            ..Default::default()
        });
        assert!(matches!(
            sender.submit(op.clone()),
            Err(EffectError::TooLarge)
        ));
        let reservation = sender.reserve_record().unwrap();
        assert!(matches!(reservation.submit(op), Err(EffectError::TooLarge)));
        assert_eq!(sender.bytes.available_permits(), Effects::BYTE_LIMIT);
        assert_eq!(sender.capacity.available_permits(), Effects::LIMIT);
    }

    #[tokio::test]
    async fn record_reservations_release_unused_bytes_before_queueing() {
        let (sender, _effects) = Effects::channel();
        let reservation = sender.reserve_record().unwrap();
        assert_eq!(
            sender.bytes.available_permits(),
            Effects::BYTE_LIMIT - Effects::MAX_PAYLOAD
        );
        let op = Fixture::op();
        let size = op.encoded_len();
        reservation.submit(op).unwrap().detach();
        assert_eq!(sender.bytes.available_permits(), Effects::BYTE_LIMIT - size);
    }

    #[tokio::test]
    async fn capacity_covers_queued_and_sent_requests_until_terminal_answer() {
        let (sender, mut effects) = Effects::channel();
        let mut receipts = Vec::new();
        for stream in 0..Effects::LIMIT {
            receipts.push(sender.submit(Fixture::op()).unwrap());
            let request = effects.recv().await.unwrap();
            assert!(effects.begin(stream as u64, request).unwrap().is_some());
        }
        assert!(matches!(
            sender.submit(Fixture::op()),
            Err(EffectError::Full)
        ));
        effects.answer(0, &Fixture::success()).unwrap();
        assert!(receipts.remove(0).wait().await.is_ok());
        sender.submit(Fixture::op()).unwrap().detach();
        assert!(matches!(
            sender.submit(Fixture::op()),
            Err(EffectError::Full)
        ));
    }

    #[tokio::test]
    async fn disconnection_completes_queued_and_sent_receipts_and_closes_admission() {
        let (sender, mut effects) = Effects::channel();
        let sent = sender.submit(Fixture::op()).unwrap();
        let request = effects.recv().await.unwrap();
        effects.begin(1, request).unwrap();
        let queued = sender.submit(Fixture::op()).unwrap();
        drop(effects);
        assert!(matches!(sent.wait().await, Err(EffectError::Closed)));
        assert!(matches!(queued.wait().await, Err(EffectError::Closed)));
        assert!(matches!(
            sender.submit(Fixture::op()),
            Err(EffectError::Closed)
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_reports_uncertainty_and_retains_wire_capacity_until_late_answer() {
        let (sender, mut effects) = Effects::channel();
        let receipt = sender.submit(Fixture::op()).unwrap();
        let request = effects.recv().await.unwrap();
        effects.begin(1, request).unwrap();
        tokio::time::advance(Effects::TIMEOUT).await;
        effects.expire().unwrap();
        assert!(matches!(receipt.wait().await, Err(EffectError::Timeout)));
        assert_eq!(sender.capacity.available_permits(), Effects::LIMIT - 1);
        assert!(effects.deadline().is_none());
        let refusal = omega_proto::Refusal::denied("late refusal").frame(1);
        let Some(omega_proto::omega::frame::Body::Result(answer)) = refusal.body else {
            panic!()
        };
        assert!(effects.answer(1, &answer).unwrap());
        assert_eq!(sender.capacity.available_permits(), Effects::LIMIT);
    }

    #[tokio::test(start_paused = true)]
    async fn expired_queued_effects_are_never_sent() {
        let (sender, mut effects) = Effects::channel();
        let receipt = sender.submit(Fixture::op()).unwrap();
        tokio::time::advance(Effects::TIMEOUT).await;
        let request = effects.recv().await.unwrap();
        assert!(effects.begin(1, request).unwrap().is_none());
        assert!(matches!(receipt.wait().await, Err(EffectError::Timeout)));
        assert_eq!(sender.capacity.available_permits(), Effects::LIMIT);
    }

    #[tokio::test]
    async fn observed_refusals_preserve_codes_but_detached_refusals_fail_loud() {
        let (sender, mut effects) = Effects::channel();
        let receipt = sender.submit(Fixture::op()).unwrap();
        let request = effects.recv().await.unwrap();
        effects.begin(1, request).unwrap();
        let refusal = omega_proto::Refusal::denied("no grant").frame(1);
        let Some(omega_proto::omega::frame::Body::Result(answer)) = refusal.body else {
            panic!()
        };
        effects.answer(1, &answer).unwrap();
        assert!(
            matches!(receipt.wait().await, Err(EffectError::Refused(error)) if error.code == ErrorCode::PermissionDenied)
        );
        sender.submit(Fixture::op()).unwrap().detach();
        let request = effects.recv().await.unwrap();
        effects.begin(3, request).unwrap();
        assert!(matches!(
            effects.answer(3, &answer),
            Err(EffectError::Refused(_))
        ));
    }
}
