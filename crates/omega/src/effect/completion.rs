use omega_proto::{Refusal, omega::Value};
use tokio::sync::oneshot;

/// Submission succeeds only when both queue and in-flight capacity are available.
pub type Submission = Result<Receipt, EffectError>;

/// The daemon's terminal answer to one effect.
pub type Completion = Result<Option<Value>, EffectError>;

/// An admission or completion failure. A timeout does not undo an external action.
#[derive(Debug, Clone, thiserror::Error)]
pub enum EffectError {
    #[error("effect exceeds payload limit")]
    TooLarge,
    #[error("effect capacity exhausted")]
    Full,
    #[error("effect runtime disconnected")]
    Closed,
    #[error("effect deadline elapsed; execution may have started")]
    Timeout,
    #[error("{0}")]
    Refused(#[from] Refusal),
    #[error("unexpected effect response")]
    UnexpectedResponse,
}
impl EffectError {
    pub(crate) fn refusal(&self) -> Refusal {
        match self {
            Self::Refused(refusal) => refusal.clone(),
            Self::Full => Refusal::exhausted(self.to_string()),
            Self::TooLarge => Refusal::too_large(self.to_string()),
            Self::Closed => Refusal::unavailable(self.to_string()),
            Self::Timeout => Refusal::deadline(self.to_string()),
            Self::UnexpectedResponse => Refusal::precondition(self.to_string()),
        }
    }
}

/// Observe one admitted effect explicitly.
/// Dropping a receipt does not cancel the effect. An unobserved failure ends the
/// runtime, so detached effects cannot fail silently.
#[derive(Debug)]
#[must_use = "forward, await, poll, or explicitly detach the effect receipt"]
pub struct Receipt<T = Option<Value>> {
    decode: fn(Option<Value>) -> Result<T, EffectError>,
    pub(crate) receiver: oneshot::Receiver<Completion>,
}
impl Receipt {
    pub(crate) fn raw(receiver: oneshot::Receiver<Completion>) -> Self {
        Self {
            receiver,
            decode: Ok,
        }
    }
    fn decoded<T>(self, decode: fn(Option<Value>) -> Result<T, EffectError>) -> Receipt<T> {
        Receipt {
            receiver: self.receiver,
            decode,
        }
    }
}
impl<T> Receipt<T> {
    /// Wait without blocking the SDK's connection loop.
    ///
    /// ```no_run
    /// # async fn example(notify: &omega::platform::notification::Notify) -> Result<(), omega::effect::EffectError> {
    /// notify.send("Finished").receipt()?.wait().await?;
    /// # Ok(()) }
    /// ```
    pub async fn wait(self) -> Result<T, EffectError> {
        self.receiver
            .await
            .unwrap_or(Err(EffectError::Closed))
            .and_then(self.decode)
    }

    /// Take a completed answer, or return `None` while it is pending.
    /// A receipt is consumed logically by its first completed poll.
    ///
    /// ```no_run
    /// # fn example(notify: &omega::platform::notification::Notify) -> Result<(), omega::effect::EffectError> {
    /// let mut receipt = notify.send("Started").receipt()?;
    /// if let Some(result) = receipt.try_complete() { result?; }
    /// # Ok(()) }
    /// ```
    pub fn try_complete(&mut self) -> Option<Result<T, EffectError>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result.and_then(self.decode)),
            Err(oneshot::error::TryRecvError::Empty) => None,
            Err(oneshot::error::TryRecvError::Closed) => Some(Err(EffectError::Closed)),
        }
    }

    /// Leave failure reporting to the runtime. This does not cancel execution.
    ///
    /// ```no_run
    /// # fn example(notify: &omega::platform::notification::Notify) -> Result<(), omega::effect::EffectError> {
    /// notify.send("Started").receipt()?.detach();
    /// # Ok(()) }
    /// ```
    pub fn detach(self) {}
}

/// An admitted operation. Awaiting it reports terminal success or failure.
/// Admission happens when the handle is called, including local record updates.
///
/// ```no_run
/// # async fn example(session: &omega::platform::session::Session) -> Result<(), omega::Error> {
/// session.lock().await?;
/// # Ok(()) }
/// ```
#[derive(Debug)]
#[must_use = "await the effect or explicitly take its receipt"]
pub struct Effect<T = ()> {
    submission: Result<Receipt<T>, EffectError>,
}

impl Effect {
    pub(crate) fn new(submission: Submission) -> Self {
        Self::decoded(submission, |_| Ok(()))
    }
}
impl<T> Effect<T> {
    pub(crate) fn decoded(
        submission: Submission,
        decode: fn(Option<Value>) -> Result<T, EffectError>,
    ) -> Self {
        Self {
            submission: submission.map(|receipt| receipt.decoded(decode)),
        }
    }
    /// Take admission and completion ownership for manual polling or detachment.
    ///
    /// ```no_run
    /// # fn example(session: &omega::platform::session::Session) -> Result<(), omega::effect::EffectError> {
    /// session.lock().receipt()?.detach();
    /// # Ok(()) }
    /// ```
    pub fn receipt(self) -> Result<Receipt<T>, EffectError> {
        self.submission
    }
}

impl<T> std::future::Future for Effect<T> {
    type Output = Result<T, crate::Error>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match &mut self.submission {
            Err(error) => std::task::Poll::Ready(Err(error.clone().into())),
            Ok(receipt) => std::pin::Pin::new(&mut receipt.receiver)
                .poll(context)
                .map(|answer| {
                    answer
                        .unwrap_or(Err(EffectError::Closed))
                        .and_then(receipt.decode)
                        .map_err(Into::into)
                }),
        }
    }
}
