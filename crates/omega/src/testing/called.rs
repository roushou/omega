//! What a command did.

use omega_proto::Values;
use omega_proto::omega::{Value, invoke};

use crate::Input;
use crate::surface::{Args, Command};
use crate::testing::state::State;

/// What a command did: what it answered, and what it asked the machine to do.
#[derive(Debug)]
pub struct Called<T = ()> {
    pub answer: Result<T, crate::Error>,
    /// The effects it queued, in order. A command that was supposed to lock
    /// the screen and did not is a command that failed.
    pub effects: Vec<invoke::Op>,
}

impl Called {
    /// Invoke a command with a typed input and collect its effects.
    pub async fn of<C: Command>(state: &State, input: C::Input) -> Called<C::Output> {
        Self::raw::<C>(state, input.encode()).await
    }

    /// Build a command against some state, call it, and collect both halves.
    /// Collected effects receive simulated success; use `TestDaemon` for wire refusals.
    ///
    /// ```
    /// # tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
    /// use omega::Command;
    /// use omega::testing::{Called, State};
    /// #[derive(omega::Command)]
    /// struct Lock { session: omega::effect::Session }
    /// impl Command for Lock {
    ///     type Input = ();
    ///     type Output = ();
    ///     async fn call(&self, _: ()) -> Result<(), omega::Error> {
    ///         self.session.lock().await
    ///     }
    /// }
    /// let called = Called::of::<Lock>(&State::new(), ()).await;
    /// assert!(called.answer.is_ok());
    /// assert_eq!(called.effects.len(), 1);
    /// # });
    /// ```
    pub async fn raw<C: Command>(state: &State, args: Vec<Value>) -> Called<C::Output> {
        Self::configured::<C>(state, &Values::new(), args).await
    }

    /// The same, for a unit the document configured.
    ///
    /// A command is never placed anywhere, so its unit's settings are the
    /// only settings it can have.
    pub async fn configured<C: Command>(
        state: &State,
        settings: &Values,
        args: Vec<Value>,
    ) -> Called<C::Output> {
        let (context, mut effects) = state.context();
        let command = C::build(&context, settings);
        let answer = async { command.call(C::Input::decode(Args::new(args))?).await };
        tokio::pin!(answer);

        let mut queued = Vec::new();
        let answer = loop {
            tokio::select! {
                biased;
                answer = &mut answer => break answer,
                Some(request) = effects.recv() => queued.push(request.complete(Ok(None)).expect("test effects succeed")),
            }
        };
        while let Some(request) = effects.try_recv() {
            queued.push(request.complete(Ok(None)).expect("test effects succeed"));
        }
        Called {
            answer,
            effects: queued,
        }
    }
}

impl<T> Called<T> {
    /// Whether it asked for exactly this action.
    pub fn did(&self, action: &omega_proto::omega::action::Kind) -> bool {
        self.effects.iter().any(|op| match op {
            invoke::Op::Act(act) => {
                act.action.as_ref().and_then(|action| action.kind.as_ref()) == Some(action)
            }
            _ => false,
        })
    }
}
