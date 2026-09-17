use super::task::{Execution, TaskKey};
use super::{Decoder, Events, Task, Wired};
use crate::{Args, Error, Surface, View};
use std::{
    collections::BTreeMap,
    sync::Arc,
    task::{Context as PollContext, Poll},
};

/// Type-erased surface rendering and lifecycle interface.
pub(crate) trait MountedSurface: Send {
    fn render(&mut self) -> View;
    fn lifecycle(&mut self, event: crate::surface::Lifecycle) -> Result<(), crate::Error>;
    fn mounted(&mut self) -> Result<(), crate::Error>;
    fn event(&mut self, binding: u64, args: Args) -> Result<(), crate::Error>;
    fn poll(
        &mut self,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), crate::Error>>;
}

pub(crate) struct Instance<S: Surface> {
    surface: S,
    effects: S::Effects,
    model: S::Model,
    bindings: BTreeMap<super::events::BindingId, Decoder<S::Message>>,
    tasks: tokio::task::JoinSet<S::Message>,
    work: BTreeMap<tokio::task::Id, Pending<S::Message>>,
    keys: BTreeMap<TaskKey, tokio::task::Id>,
    queued: std::collections::VecDeque<S::Message>,
    budget: Arc<tokio::sync::Semaphore>,
}
struct Pending<M> {
    key: Option<TaskKey>,
    failed: Arc<dyn Fn(Error) -> M + Send + Sync>,
    abort: tokio::task::AbortHandle,
}
impl<S: Surface> Instance<S> {
    pub(crate) fn model(&self) -> &S::Model {
        &self.model
    }
    pub(crate) fn replace_effects(&mut self, effects: S::Effects) {
        self.effects = effects;
    }

    pub(crate) fn new(
        context: &crate::runtime::context::Context,
        settings: &omega_proto::Values,
    ) -> Self {
        Self {
            surface: S::build(context, settings),
            effects: S::Effects::build(context, settings),
            model: S::Model::default(),
            bindings: BTreeMap::new(),
            tasks: tokio::task::JoinSet::new(),
            work: BTreeMap::new(),
            keys: BTreeMap::new(),
            queued: Default::default(),
            budget: context.task_budget(),
        }
    }
    fn schedule(&mut self, task: Task<S::Message>) -> Result<(), Error> {
        if task.work.len() > 16 {
            return Err(Error::invalid("an update may schedule at most 16 tasks"));
        }
        for work in task.work {
            if let Some(key) = &work.key
                && let Some(old) = self.keys.remove(key)
                && let Some(pending) = self.work.get(&old)
            {
                pending.abort.abort();
            }
            let permit = self.budget.clone().try_acquire_owned();
            if self.work.len() >= 64 || permit.is_err() {
                if self.queued.len() >= 64 {
                    return Err(Error::invalid("task failure queue exhausted"));
                }
                self.queued.push_back((work.failed)(Error::invalid(
                    "surface task capacity exhausted",
                )));
                continue;
            }
            let permit = permit.expect("checked permit");
            let handle = match work.execution {
                Execution::Async(future) => self.tasks.spawn(async move {
                    let _permit = permit;
                    future.await
                }),
                Execution::Blocking(work) => self.tasks.spawn_blocking(move || {
                    let _permit = permit;
                    work()
                }),
            };
            let id = handle.id();
            if let Some(key) = &work.key {
                self.keys.insert(key.clone(), id);
            }
            self.work.insert(
                id,
                Pending {
                    key: work.key,
                    failed: work.failed,
                    abort: handle,
                },
            );
        }
        Ok(())
    }
    pub(crate) fn message(&mut self, message: S::Message) -> Result<(), Error> {
        let task = self.surface.update(&mut self.model, message, &self.effects);
        self.schedule(task)
    }
}
impl<S: Surface> MountedSurface for Instance<S> {
    fn render(&mut self) -> View {
        let events = Events::new();
        let view = self.surface.render(&self.model, &events);
        self.bindings = events.finish();
        view
    }
    fn lifecycle(&mut self, event: super::Lifecycle) -> Result<(), Error> {
        if event == super::Lifecycle::Closed {
            self.tasks.abort_all();
            self.tasks.detach_all();
            self.work.clear();
            self.keys.clear();
            self.queued.clear();
            self.bindings.clear();
        }
        let task = self
            .surface
            .lifecycle(&mut self.model, event, &self.effects);
        if event == super::Lifecycle::Closed && !task.work.is_empty() {
            return Err(Error::invalid("closed surfaces cannot schedule work"));
        }
        self.schedule(task)
    }
    fn mounted(&mut self) -> Result<(), Error> {
        let task = self.surface.mounted(&mut self.model, &self.effects);
        self.schedule(task)
    }
    fn event(&mut self, binding: u64, args: Args) -> Result<(), Error> {
        let binding = super::events::BindingId::try_from(binding)?;
        let decoder = self.bindings.get(&binding).ok_or_else(|| {
            Error::from(omega_proto::Refusal::precondition("expired local binding"))
        })?;
        let message = decoder(args)?;
        self.message(message)
    }
    fn poll(&mut self, cx: &mut PollContext<'_>) -> Poll<Result<(), Error>> {
        if let Some(message) = self.queued.pop_front() {
            return Poll::Ready(self.message(message));
        }
        loop {
            let Poll::Ready(Some(completion)) = self.tasks.poll_join_next_with_id(cx) else {
                return Poll::Pending;
            };
            let id = match &completion {
                Ok((id, _)) => *id,
                Err(error) => error.id(),
            };
            let pending = self.work.remove(&id).expect("managed task metadata");
            if let Some(key) = &pending.key {
                if self.keys.get(key) != Some(&id) {
                    continue;
                }
                self.keys.remove(key);
            }
            let message = match completion {
                Ok((_, message)) => message,
                Err(error) => {
                    (pending.failed)(Error::invalid(format!("surface task failed: {error}")))
                }
            };
            return Poll::Ready(self.message(message));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(omega::Surface)]
    struct Worker {}
    enum Message {
        Start(
            std::sync::mpsc::Receiver<()>,
            tokio::sync::oneshot::Sender<()>,
        ),
        Done(Result<(), Error>),
    }
    impl Surface for Worker {
        type Model = ();
        type Message = Message;
        type Effects = ();
        fn render(&self, _: &(), _: &Events<Message>) -> View {
            View::default()
        }
        fn update(&self, _: &mut (), message: Message, _: &()) -> Task<Message> {
            match message {
                Message::Start(release, started) => Task::blocking(
                    "worker",
                    move || {
                        let _ = started.send(());
                        release.recv().unwrap();
                        Ok(())
                    },
                    Message::Done,
                ),
                Message::Done(result) => {
                    result.unwrap();
                    Task::none()
                }
            }
        }
    }
    #[tokio::test]
    async fn closing_a_running_worker_does_not_release_its_budget_early() {
        let (sender, _effects) = crate::effect::queue::Effects::channel();
        let context = crate::runtime::context::Context::new(&Default::default(), sender);
        let budget = context.task_budget();
        let mut instance = Instance::<Worker>::new(&context, &Default::default());
        let (release, wait) = std::sync::mpsc::channel();
        let (started, ready) = tokio::sync::oneshot::channel();
        instance.message(Message::Start(wait, started)).unwrap();
        ready.await.unwrap();
        instance.lifecycle(super::super::Lifecycle::Closed).unwrap();
        assert_eq!(budget.available_permits(), 255);
        release.send(()).unwrap();
        let _all =
            tokio::time::timeout(std::time::Duration::from_secs(5), budget.acquire_many(256))
                .await
                .unwrap()
                .unwrap();
        assert!(instance.work.is_empty());
    }
}
