use super::{Drawn, State};
use crate::surface::instance::{Instance, MountedSurface};
use crate::{Args, Error, Input, Surface};

/// The production model, binding and task runtime without a daemon or live services.
///
/// Use a Tokio test runtime for tasks; paused Tokio time makes delays deterministic.
/// Effects enter an isolated queue and never fall through to the live desktop.
pub struct SurfaceHarness<S: Surface> {
    instance: Instance<S>,
    context: crate::runtime::context::Context,
    effects: crate::effect::queue::Effects,
    revision: u64,
}
impl<S: Surface> SurfaceHarness<S> {
    fn identity() -> omega_proto::instance::InstanceKey {
        omega_proto::instance::InstanceKey {
            id: "fixture"
                .parse::<omega_proto::instance::InstanceId>()
                .expect("fixture id"),
            incarnation: "fixture"
                .parse::<omega_proto::instance::IncarnationId>()
                .expect("fixture incarnation"),
        }
    }
    pub fn new(state: &State) -> Result<Self, Error> {
        Self::configured(state, &omega_proto::Values::new())
    }
    pub fn configured(state: &State, settings: &omega_proto::Values) -> Result<Self, Error> {
        let (context, effects) = state.context();
        let context = context.for_instance(Self::identity());
        let mut instance = Instance::<S>::new(&context, settings);
        instance.mounted()?;
        Ok(Self {
            instance,
            context,
            effects,
            revision: 1,
        })
    }
    /// Inject behavior dependencies before initialization, for deterministic services.
    pub fn with_effects(
        state: &State,
        settings: &omega_proto::Values,
        effects: S::Effects,
    ) -> Result<Self, Error> {
        let (context, queue) = state.context();
        let context = context.for_instance(Self::identity());
        let mut instance = Instance::<S>::new(&context, settings);
        instance.replace_effects(effects);
        instance.mounted()?;
        Ok(Self {
            instance,
            context,
            effects: queue,
            revision: 1,
        })
    }
    pub fn lifecycle(&mut self, event: crate::surface::Lifecycle) -> Result<(), Error> {
        self.instance.lifecycle(event)
    }
    pub fn model(&self) -> &S::Model {
        self.instance.model()
    }
    pub fn draw(&mut self) -> Drawn {
        if !self.context.holds(&S::required_topics()) {
            return Drawn::of_view(crate::View::default());
        }
        Drawn {
            tree: self.instance.render().expect("surface render failed"),
        }
    }
    pub fn send(&mut self, message: S::Message) -> Result<(), Error> {
        self.instance.message(message)
    }
    /// Dispatch a local binding from the supplied render; obsolete and foreign bindings
    /// are refused. Disabled/busy ancestors block interaction. Ambiguous node keys,
    /// missing events, and mixed local/command bindings are refused before dispatch.
    pub fn interact<I: Input>(
        &mut self,
        drawn: &Drawn,
        key: &str,
        event: &str,
        input: I,
    ) -> Result<(), Error> {
        match omega_proto::Interaction::resolve(drawn.tree(), key, event)? {
            omega_proto::Interaction::Local(binding) => self
                .instance
                .event(binding.get(), Args::new(input.encode())),
            omega_proto::Interaction::Command { .. } => {
                Err(Error::invalid("expected a local binding"))
            }
        }
    }
    /// Complete the next queued external effect with an explicit fixture outcome.
    /// Returns the operation for assertions; no real backend is contacted.
    pub async fn complete_effect(
        &mut self,
        outcome: crate::effect::Completion,
    ) -> Result<omega_proto::omega::invoke::Op, Error> {
        let request = self
            .effects
            .recv()
            .await
            .ok_or_else(|| Error::invalid("fixture effect queue closed"))?;
        Ok(request.complete(outcome)?)
    }
    /// Poll the production task scheduler without imposing a fixture clock.
    pub fn poll_task(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Error>> {
        self.instance.poll(cx)
    }
    /// Take an already queued effect for inspection and explicit resolution.
    /// No backend is contacted, including when the request is dropped.
    pub fn take_effect(&mut self) -> Option<CapturedEffect> {
        self.effects.try_recv().map(CapturedEffect)
    }
    /// Deliver the next managed task completion using the production scheduler.
    pub async fn complete(&mut self) -> Result<(), Error> {
        std::future::poll_fn(|cx| self.instance.poll(cx)).await
    }
    pub fn state(&mut self, state: &State) {
        self.revision += 1;
        let mut topics = state.snapshot().topics;
        for topic in &mut topics {
            topic.revision = self.revision;
        }
        self.context
            .apply(&omega_proto::omega::StatePatch { topics });
    }
}
impl<S: Surface> std::fmt::Debug for SurfaceHarness<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceHarness").finish_non_exhaustive()
    }
}

/// An isolated operation awaiting a fixture-defined outcome.
pub struct CapturedEffect(crate::effect::queue::Request);
impl CapturedEffect {
    /// Inspect the operation before deciding its simulated outcome.
    pub fn operation(&self) -> &omega_proto::omega::invoke::Op {
        self.0.operation()
    }
    /// Resolve exactly once. Dropping the capture closes its isolated receipt.
    pub fn complete(
        self,
        outcome: crate::effect::Completion,
    ) -> Result<(), crate::effect::EffectError> {
        self.0.complete(outcome).map(|_| ())
    }
}
impl std::fmt::Debug for CapturedEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapturedEffect").finish_non_exhaustive()
    }
}
