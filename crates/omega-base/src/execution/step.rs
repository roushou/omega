use super::{Description, Progress};
use std::{future::Future, pin::Pin};

/// A replaceable operation. Synchronous work can return from an `async fn`
/// without awaiting. Inputs and implementations are owned by one run; futures
/// are polled by the caller and need not be Send. Errors stay typed.
pub trait Operation<Input> {
    type Output;
    type Error: std::fmt::Display;

    fn execute(
        self,
        input: Input,
        progress: &mut Progress<'_>,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>>;
}

pub(super) type Pending<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
pub(super) trait Invoke<I, O, E>: 'static {
    fn invoke<'a>(
        self: Box<Self>,
        input: I,
        progress: &'a mut Progress<'_>,
    ) -> Pending<'a, Result<O, E>>;
}

impl<I: 'static, O: 'static, E: 'static, S> Invoke<I, O, E> for S
where
    S: Operation<I, Output = O, Error = E> + 'static,
{
    fn invoke<'a>(
        self: Box<Self>,
        input: I,
        progress: &'a mut Progress<'_>,
    ) -> Pending<'a, Result<O, E>> {
        Box::pin(async move { (*self).execute(input, progress).await })
    }
}

/// A named, typed slot in a pipeline. A new slot has no implementation and fails
/// explicitly if reached. Replacing it preserves its identity and position.
/// Values crossing steps are owned (`'static`); their concrete types are checked
/// by composition. Only implementation dispatch is erased, never input values.
pub struct Step<I, O, E> {
    pub(super) description: Description,
    pub(super) implementation: Option<Box<dyn Invoke<I, O, E>>>,
}

impl<I: 'static, O: 'static, E: std::fmt::Display + 'static> Step<I, O, E> {
    pub fn new(id: &'static str, title: impl Into<String>) -> Self {
        Self {
            description: Description::new(id, title),
            implementation: None,
        }
    }

    pub fn using(mut self, operation: impl Operation<I, Output = O, Error = E> + 'static) -> Self {
        self.replace(operation);
        self
    }

    /// Replace behavior without changing identity or type contracts.
    ///
    /// ```compile_fail
    /// use omega_base::execution::{Step, testing::Stub};
    /// let mut step = Step::<String, usize, String>::new("length", "measure text");
    /// step.replace(Stub::<String, bool, String>::returning(true));
    /// ```
    pub fn replace(&mut self, operation: impl Operation<I, Output = O, Error = E> + 'static) {
        self.implementation = Some(Box::new(operation));
    }
}

/// Carry unrelated owned context alongside an operation's typed input/output.
/// The operation receives only its own input; the context passes through intact.
#[derive(Debug)]
pub struct Carry<S>(pub S);

impl<C, I, S: Operation<I>> Operation<(C, I)> for Carry<S> {
    type Output = (C, S::Output);
    type Error = S::Error;

    async fn execute(
        self,
        (context, input): (C, I),
        progress: &mut Progress<'_>,
    ) -> Result<Self::Output, Self::Error> {
        self.0
            .execute(input, progress)
            .await
            .map(|output| (context, output))
    }
}

impl<I: 'static, O: 'static, E: std::fmt::Display + 'static> Step<I, O, E> {
    /// Preserve unrelated context across this slot, including any replacement.
    pub fn carrying<C: 'static>(self) -> Step<(C, I), (C, O), E> {
        Step {
            description: self.description,
            implementation: self.implementation.map(|operation| {
                Box::new(Carried { operation }) as Box<dyn Invoke<(C, I), (C, O), E>>
            }),
        }
    }
}

struct Carried<I, O, E> {
    operation: Box<dyn Invoke<I, O, E>>,
}

impl<C: 'static, I: 'static, O: 'static, E: 'static> Invoke<(C, I), (C, O), E>
    for Carried<I, O, E>
{
    fn invoke<'a>(
        self: Box<Self>,
        (context, input): (C, I),
        progress: &'a mut Progress<'_>,
    ) -> Pending<'a, Result<(C, O), E>> {
        Box::pin(async move {
            self.operation
                .invoke(input, progress)
                .await
                .map(|output| (context, output))
        })
    }
}

impl<I, O, E> std::fmt::Debug for Step<I, O, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Step")
            .field("description", &self.description)
            .field("configured", &self.implementation.is_some())
            .finish()
    }
}
