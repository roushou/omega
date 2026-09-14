use crate::Error;
use std::{future::Future, pin::Pin, sync::Arc};

/// Work owned by an instance. Dropping the instance cancels delivery of results.
///
/// Replaceable tasks invalidate previous results before cancellation. Cancellation
/// cannot undo an external effect already admitted; its receipt remains owned by
/// the effect runtime and is never replayed automatically.
pub struct Task<M> {
    pub(crate) work: Vec<Work<M>>,
}
pub(crate) struct Work<M> {
    pub key: Option<TaskKey>,
    pub execution: Execution<M>,
    pub failed: Arc<dyn Fn(Error) -> M + Send + Sync>,
}
impl<M> Default for Task<M> {
    fn default() -> Self {
        Self { work: Vec::new() }
    }
}
impl<M: Send + 'static> Task<M> {
    pub fn none() -> Self {
        Self::default()
    }
    /// Run work and map success, failure, or a task panic into a local message.
    pub fn perform<T: Send + 'static>(
        future: impl Future<Output = Result<T, Error>> + Send + 'static,
        completed: impl Fn(Result<T, Error>) -> M + Send + Sync + 'static,
    ) -> Self {
        let completed = Arc::new(completed);
        let failed = completed.clone();
        Self {
            work: vec![Work {
                key: None,
                execution: Execution::Async(Box::pin(async move { completed(future.await) })),
                failed: Arc::new(move |error| failed(Err(error))),
            }],
        }
    }
    /// Replace this instance's work under a nonempty key of at most 128 bytes.
    pub fn replace<T: Send + 'static>(
        key: impl Into<String>,
        future: impl Future<Output = Result<T, Error>> + Send + 'static,
        completed: impl Fn(Result<T, Error>) -> M + Send + Sync + 'static,
    ) -> Self {
        let key = match TaskKey::parse(key.into()) {
            Ok(key) => key,
            Err(error) => return Self::perform(async move { Err(error) }, completed),
        };
        let mut task = Self::perform(future, completed);
        task.work[0].key = Some(key);
        task
    }
    /// Run replaceable CPU/blocking work off the UI runtime. A started worker
    /// keeps its concurrency permit until it exits, even after cancellation.
    pub fn blocking<T: Send + 'static>(
        key: impl Into<String>,
        work: impl FnOnce() -> Result<T, Error> + Send + 'static,
        completed: impl Fn(Result<T, Error>) -> M + Send + Sync + 'static,
    ) -> Self {
        let key = match TaskKey::parse(key.into()) {
            Ok(key) => key,
            Err(error) => return Self::perform(async move { Err(error) }, completed),
        };
        let completed = Arc::new(completed);
        let failed = completed.clone();
        Self {
            work: vec![Work {
                key: Some(key),
                execution: Execution::Blocking(Box::new(move || completed(work()))),
                failed: Arc::new(move |error| failed(Err(error))),
            }],
        }
    }
    /// Admit independent work from one update together.
    pub fn batch(tasks: impl IntoIterator<Item = Self>) -> Self {
        Self {
            work: tasks.into_iter().flat_map(|task| task.work).collect(),
        }
    }
}

impl<M> std::fmt::Debug for Task<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Task")
            .field("count", &self.work.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TaskKey(String);
impl TaskKey {
    fn parse(key: String) -> Result<Self, Error> {
        if key.is_empty() || key.len() > 128 || key.contains('\0') {
            return Err(Error::invalid(
                "task key must contain 1–128 bytes without NUL",
            ));
        }
        Ok(Self(key))
    }
}

pub(crate) enum Execution<M> {
    Async(Pin<Box<dyn Future<Output = M> + Send>>),
    Blocking(Box<dyn FnOnce() -> M + Send>),
}
