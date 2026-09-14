//! Behavior triggered by external state transitions.
use crate::wiring::Wired;
use omega_proto::omega::Event;

/// Handle a state-transition event, such as external power disconnecting.
/// Reactions may hold effect handles. They run on matching events, not on
/// every reading update.
pub trait Reaction: Wired {
    fn fire(&self, event: &Event);
}
