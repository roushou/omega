//! Behavior triggered by external state transitions.
use crate::wiring::Wired;
use omega_proto::omega::Event;

/// Something that happens.
///
/// A reaction runs when the daemon reports a transition — the AC was
/// unplugged, the battery went low — rather than on every state change. Reactions may hold effects.
pub trait Reaction: Wired {
    fn fire(&self, event: &Event);
}
