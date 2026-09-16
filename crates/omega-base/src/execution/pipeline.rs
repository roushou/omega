use super::{Description, Observer, Report, Runner, Step, step::Pending};
use std::fmt::{self, Display};

/// The failed step and its original operation error, or an unconfigured slot.
#[derive(Debug)]
pub struct Failure<E> {
    pub step: Description,
    pub cause: FailureCause<E>,
}

#[derive(Debug)]
pub enum FailureCause<E> {
    Operation(E),
    Unconfigured,
}

impl<E: Display> Display for Failure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.cause {
            FailureCause::Operation(error) => write!(f, "{}: {error}", self.step.title),
            FailureCause::Unconfigured => {
                write!(f, "{}: no implementation configured", self.step.title)
            }
        }
    }
}

/// Result and attempt history from a finished run. Dropped runs report their
/// interruption to the observer; they cannot return a Run value.
#[derive(Debug)]
pub struct Run<O, E> {
    pub result: Result<O, Failure<E>>,
    pub reports: Vec<Report>,
}

/// An inert, typed sequence. `then` connects one step's output to the next input.
/// `run` consumes both the declaration and its owned input, stops at the first
/// failure, and performs no retries or compensation.
///
/// ```
/// use omega_base::execution::{Operation, Pipeline, Progress, Step};
/// struct Length;
/// impl Operation<String> for Length {
///     type Output = usize;
///     type Error = std::convert::Infallible;
///     async fn execute(self, text: String, _: &mut Progress<'_>) -> Result<usize, Self::Error> {
///         Ok(text.len())
///     }
/// }
/// let pipeline = Pipeline::<String, String, std::convert::Infallible>::new()
///     .then(Step::new("length", "measure text").using(Length));
/// assert_eq!(pipeline.steps()[0].id.0, "length");
/// // In an async context: pipeline.run("hello".into(), &mut observer).await
/// ```
///
/// Incompatible connections are compile errors:
/// ```compile_fail
/// use omega_base::execution::{Pipeline, Step};
/// let pipeline = Pipeline::<String, String, String>::new()
///     .then(Step::<usize, bool, String>::new("wrong", "expects an integer"));
/// ```
pub struct Pipeline<I, O, E> {
    node: Box<dyn Node<I, O, E>>,
    steps: Vec<Description>,
}

impl<I: 'static, E: Display + 'static> Pipeline<I, I, E> {
    pub fn new() -> Self {
        Self {
            node: Box::new(Start),
            steps: Vec::new(),
        }
    }
}

impl<I: 'static, E: Display + 'static> Default for Pipeline<I, I, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: 'static, O: 'static, E: Display + 'static> Pipeline<I, O, E> {
    pub fn then<N: 'static>(mut self, step: Step<O, N, E>) -> Pipeline<I, N, E> {
        assert!(
            !self
                .steps
                .iter()
                .any(|known| known.id == step.description.id),
            "pipeline step IDs must be unique"
        );

        self.steps.push(step.description.clone());

        Pipeline {
            node: Box::new(Then {
                previous: self.node,
                step,
            }),
            steps: self.steps,
        }
    }

    pub fn steps(&self) -> &[Description] {
        &self.steps
    }

    pub async fn run(self, input: I, observer: &mut dyn Observer) -> Run<O, E> {
        let mut runner = Runner {
            observer,
            reports: Vec::with_capacity(self.steps.len()),
        };
        let result = self.node.run(input, &mut runner).await;

        Run {
            result,
            reports: runner.reports,
        }
    }
}

trait Node<I, O, E>: 'static {
    fn run<'a>(
        self: Box<Self>,
        input: I,
        runner: &'a mut Runner<'_>,
    ) -> Pending<'a, Result<O, Failure<E>>>;
}

struct Start;

impl<I: 'static, E: 'static> Node<I, I, E> for Start {
    fn run<'a>(
        self: Box<Self>,
        input: I,
        _: &'a mut Runner<'_>,
    ) -> Pending<'a, Result<I, Failure<E>>> {
        Box::pin(async move { Ok(input) })
    }
}

struct Then<I, M, O, E> {
    previous: Box<dyn Node<I, M, E>>,
    step: Step<M, O, E>,
}

impl<I: 'static, M: 'static, O: 'static, E: Display + 'static> Node<I, O, E> for Then<I, M, O, E> {
    fn run<'a>(
        self: Box<Self>,
        input: I,
        runner: &'a mut Runner<'_>,
    ) -> Pending<'a, Result<O, Failure<E>>> {
        Box::pin(async move {
            let input = self.previous.run(input, runner).await?;

            let step = self.step;
            let mut attempt = runner.begin(step.description.clone());
            let result = match step.implementation {
                Some(operation) => operation
                    .invoke(input, &mut attempt.progress())
                    .await
                    .map_err(|error| Failure {
                        step: step.description,
                        cause: FailureCause::Operation(error),
                    }),
                None => Err(Failure {
                    step: step.description,
                    cause: FailureCause::Unconfigured,
                }),
            };

            attempt.finish(&result);
            result
        })
    }
}

impl<I, O, E> fmt::Debug for Pipeline<I, O, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pipeline")
            .field("steps", &self.steps)
            .finish_non_exhaustive()
    }
}

impl<E: std::error::Error + 'static> std::error::Error for Failure<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.cause {
            FailureCause::Operation(error) => Some(error),
            FailureCause::Unconfigured => None,
        }
    }
}
