//! What a command did.

use omega_proto::Values;
use omega_proto::omega::{Value, invoke};

use crate::surface::{Answer, Args, Command};
use crate::testing::state::State;

/// What a command did: what it answered, and what it asked the machine to do.
#[derive(Debug)]
pub struct Called {
    pub answer: Answer,
    /// The effects it queued, in order. A command that was supposed to lock
    /// the screen and did not is a command that failed.
    pub effects: Vec<invoke::Op>,
}

impl Called {
    /// Build a command against some state, call it, and collect both halves.
    pub fn of<C: Command>(state: &State, args: Vec<Value>) -> Self {
        Self::configured::<C>(state, &Values::new(), args)
    }

    /// The same, for a unit the document configured.
    ///
    /// A command is never placed anywhere, so its unit's settings are the
    /// only settings it can have.
    pub fn configured<C: Command>(state: &State, settings: &Values, args: Vec<Value>) -> Self {
        let (context, mut effects) = state.context();
        let command = C::build(&context, settings);
        let answer = command.call(Args::new(args));

        let mut queued = Vec::new();
        while let Ok(op) = effects.try_recv() {
            queued.push(op);
        }
        Self {
            answer,
            effects: queued,
        }
    }

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
