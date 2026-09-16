use super::{Observation, State};

#[derive(Debug, Clone, Copy)]
pub(super) enum Operation {
    Apply,
    Accept,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Decision {
    Apply,
    Accept,
    Restore,
    FinishRestoring,
    AlreadyRestored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DecisionError {
    State,
    Conflict,
}

pub(super) struct Policy;

impl Policy {
    /// Pure decisions: effects and journal writes happen in SavedChange.
    pub(super) fn decide(
        state: State,
        observed: Observation,
        operation: Operation,
    ) -> Result<Decision, DecisionError> {
        use Observation::{After, Before, Conflict, Unchanged};
        match operation {
            Operation::Apply => {
                if state != State::Prepared {
                    return Err(DecisionError::State);
                }
                match observed {
                    Before | Unchanged => Ok(Decision::Apply),
                    After | Conflict => Err(DecisionError::Conflict),
                }
            }
            Operation::Accept => {
                if !matches!(state, State::Applying | State::Applied) {
                    return Err(DecisionError::State);
                }
                match observed {
                    After | Unchanged => Ok(Decision::Accept),
                    Before | Conflict => Err(DecisionError::Conflict),
                }
            }
            Operation::Restore => match (state, observed) {
                (State::Restored, Before | Unchanged) => Ok(Decision::AlreadyRestored),
                (State::Restored | State::Prepared, After | Conflict) => {
                    Err(DecisionError::Conflict)
                }
                (
                    State::Prepared | State::Applying | State::Applied | State::Restoring,
                    Before | Unchanged,
                ) => Ok(Decision::FinishRestoring),
                (State::Applying | State::Applied | State::Restoring, After) => {
                    Ok(Decision::Restore)
                }
                (State::Applying | State::Applied | State::Restoring, Conflict) => {
                    Err(DecisionError::Conflict)
                }
            },
        }
    }
}
