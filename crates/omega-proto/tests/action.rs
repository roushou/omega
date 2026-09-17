//! The action table: every kind, and what it costs.

use std::collections::HashSet;

use omega_proto::ActionKind;
use omega_proto::omega::Capability;

#[test]
fn every_action_states_what_it_costs() {
    // The escape hatch and the power button are not the same permission.
    assert_eq!(ActionKind::RunCommand.cost(), Some(Capability::Spawn));
    assert_eq!(ActionKind::Shutdown.cost(), Some(Capability::SystemControl));
    assert_eq!(ActionKind::SetBacklight.cost(), Some(Capability::Backlight));
    assert_eq!(ActionKind::Notify.cost(), Some(Capability::Notify));

    // Actions that only move a plugin's own windows around cost nothing extra.
    assert_eq!(ActionKind::ToggleFullscreen.cost(), None);
}

#[test]
fn every_action_is_named_once() {
    // Action names must be unique for reverse lookup.
    let names: HashSet<&str> = ActionKind::ALL.iter().map(|kind| kind.name()).collect();
    assert_eq!(names.len(), ActionKind::ALL.len());
}
