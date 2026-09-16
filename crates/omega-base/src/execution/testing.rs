//! Test replacements with no fallback to production effects.

use super::{Description, Detail, Observer, Operation, Progress, Report};
use std::{future::pending, marker::PhantomData};

/// Returns one configured outcome. Input capture can be implemented in a custom
/// Operation when a test needs to inspect domain values or coordinate completion.
#[derive(Debug)]
pub struct Stub<I, O, E> {
    result: Result<O, E>,
    input: PhantomData<fn(I)>,
}

impl<I, O, E> Stub<I, O, E> {
    pub fn returning(output: O) -> Self {
        Self {
            result: Ok(output),
            input: PhantomData,
        }
    }

    pub fn failing(error: E) -> Self {
        Self {
            result: Err(error),
            input: PhantomData,
        }
    }
}

impl<I, O, E: std::fmt::Display> Operation<I> for Stub<I, O, E> {
    type Output = O;
    type Error = E;

    async fn execute(self, _: I, _: &mut Progress<'_>) -> Result<O, E> {
        self.result
    }
}

/// A step that stays pending until its run is dropped. Useful for interruption
/// tests without clocks, executors, or real effects.
#[derive(Debug)]
pub struct Pending<I, O, E>(PhantomData<fn(I) -> Result<O, E>>);

impl<I, O, E> Default for Pending<I, O, E> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<I, O, E: std::fmt::Display> Operation<I> for Pending<I, O, E> {
    type Output = O;
    type Error = E;

    async fn execute(self, _: I, _: &mut Progress<'_>) -> Result<O, E> {
        pending().await
    }
}

/// Records observer updates in arrival order, including interrupted runs.
#[derive(Debug, Default)]
pub struct Recorder {
    pub events: Vec<Event>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Outcome(Report),
    Detail { step: Description, detail: Detail },
}

impl Observer for Recorder {
    fn observe(&mut self, report: &Report) {
        self.events.push(Event::Outcome(report.clone()));
    }

    fn detail(&mut self, step: &Description, detail: &Detail) {
        self.events.push(Event::Detail {
            step: step.clone(),
            detail: detail.clone(),
        });
    }
}

/// A successful no-op that returns its input unchanged.
#[derive(Debug)]
pub struct Pass<E>(PhantomData<fn() -> E>);

impl<E> Default for Pass<E> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<I, E: std::fmt::Display> Operation<I> for Pass<E> {
    type Output = I;
    type Error = E;

    async fn execute(self, input: I, _: &mut Progress<'_>) -> Result<I, E> {
        Ok(input)
    }
}

/// Hold an operation before it starts until the test releases it. Cancellation
/// drops the owned input and operation normally; no worker or timer is spawned.
#[derive(Debug)]
pub struct Gate<S> {
    operation: S,
    signal: std::sync::Arc<std::sync::Mutex<Signal>>,
}

/// One-shot release for a gated operation. Dropping it without release makes a
/// polled gate panic, so a forgotten test completion cannot hang silently.
#[derive(Debug)]
pub struct Release {
    signal: std::sync::Arc<std::sync::Mutex<Signal>>,
    released: bool,
}

#[derive(Debug, Default)]
struct Signal {
    released: bool,
    abandoned: bool,
    waker: Option<std::task::Waker>,
}

impl<S> Gate<S> {
    pub fn new(operation: S) -> (Self, Release) {
        let signal = std::sync::Arc::new(std::sync::Mutex::new(Signal::default()));
        (
            Self {
                operation,
                signal: signal.clone(),
            },
            Release {
                signal,
                released: false,
            },
        )
    }
}

impl Release {
    pub fn release(mut self) {
        self.released = true;
        let waker = {
            let mut signal = self.signal.lock().expect("test gate poisoned");
            signal.released = true;
            signal.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl Drop for Release {
    fn drop(&mut self) {
        if !self.released {
            let waker = {
                let mut signal = self.signal.lock().expect("test gate poisoned");
                signal.abandoned = true;
                signal.waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }
}

impl<I, S: Operation<I>> Operation<I> for Gate<S> {
    type Output = S::Output;
    type Error = S::Error;

    async fn execute(
        self,
        input: I,
        progress: &mut Progress<'_>,
    ) -> Result<Self::Output, Self::Error> {
        std::future::poll_fn(|context| {
            let mut signal = self.signal.lock().expect("test gate poisoned");
            assert!(
                !signal.abandoned,
                "test gate release was dropped without completion"
            );
            if signal.released {
                std::task::Poll::Ready(())
            } else {
                signal.waker = Some(context.waker().clone());
                std::task::Poll::Pending
            }
        })
        .await;
        self.operation.execute(input, progress).await
    }
}
