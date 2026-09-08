//! What each op costs, in one table.
//!
//! Every op the wire defines is listed here with what it demands, so an op
//! with no row is refused as unimplemented and a handler can never become
//! reachable without a policy. Its own file because it is the contract:
//! reading what a peer may do should not mean scrolling past the code that
//! does it.

use omega_proto::omega::{Capability, SurfaceKind, invoke};

use crate::session::admission::Role;

/// Every op in `wire.proto`. Exhaustive by construction: adding an op to the
/// schema fails to compile here until its kind is named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    GetState,
    SetState,
    Subscribe,
    Unsubscribe,
    Act,
    EmitEvent,
    PublishView,
    RestartUnit,
    AdoptUnit,
    CallCommand,
    RenderWidget,
    CallAgentTool,
}

impl OpKind {
    pub fn of(op: &invoke::Op) -> Self {
        match op {
            invoke::Op::GetState(_) => Self::GetState,
            invoke::Op::SetState(_) => Self::SetState,
            invoke::Op::Subscribe(_) => Self::Subscribe,
            invoke::Op::Unsubscribe(_) => Self::Unsubscribe,
            invoke::Op::Act(_) => Self::Act,
            invoke::Op::EmitEvent(_) => Self::EmitEvent,
            invoke::Op::PublishView(_) => Self::PublishView,
            invoke::Op::RestartUnit(_) => Self::RestartUnit,
            invoke::Op::AdoptUnit(_) => Self::AdoptUnit,
            invoke::Op::CallCommand(_) => Self::CallCommand,
            invoke::Op::RenderWidget(_) => Self::RenderWidget,
            invoke::Op::CallAgentTool(_) => Self::CallAgentTool,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::GetState => "GetState",
            Self::SetState => "SetState",
            Self::Subscribe => "Subscribe",
            Self::Unsubscribe => "Unsubscribe",
            Self::Act => "Act",
            Self::EmitEvent => "EmitEvent",
            Self::PublishView => "PublishView",
            Self::RestartUnit => "RestartUnit",
            Self::AdoptUnit => "AdoptUnit",
            Self::CallCommand => "CallCommand",
            Self::RenderWidget => "RenderWidget",
            Self::CallAgentTool => "CallAgentTool",
        }
    }

    /// The surface an op targets, if it targets one. Exhaustive for the same
    /// reason: a new surface-scoped op must state that it is one.
    pub(super) fn target_surface(op: &invoke::Op) -> Option<&str> {
        match op {
            invoke::Op::PublishView(publish) => Some(&publish.surface_id),
            invoke::Op::RenderWidget(render) => Some(&render.surface_id),
            invoke::Op::GetState(_)
            | invoke::Op::SetState(_)
            | invoke::Op::Subscribe(_)
            | invoke::Op::Unsubscribe(_)
            | invoke::Op::Act(_)
            | invoke::Op::EmitEvent(_)
            | invoke::Op::RestartUnit(_)
            | invoke::Op::AdoptUnit(_)
            | invoke::Op::CallCommand(_)
            | invoke::Op::CallAgentTool(_) => None,
        }
    }
}

/// What an op requires of the peer that sent it.
pub(super) struct OpPolicy {
    pub(super) kind: OpKind,
    /// Who this op is served to. A unit does not manage its neighbours, and
    /// an operator does not act as a unit — but some ops belong to both.
    pub(super) roles: &'static [Role],
    /// Capabilities the unit's manifest must grant.
    pub(super) capabilities: &'static [Capability],
    /// The kind the targeted surface must be declared as. Publishing to a
    /// surface a unit did not declare is another unit's business.
    pub(super) surface: Option<SurfaceKind>,
}

/// The ops this daemon serves, and what each demands. Deny by default:
/// anything absent is refused.
pub(super) const POLICY: &[OpPolicy] = &[
    OpPolicy {
        kind: OpKind::PublishView,
        roles: &[Role::Unit],
        capabilities: &[],
        surface: Some(SurfaceKind::Widget),
    },
    OpPolicy {
        kind: OpKind::GetState,
        roles: &[Role::Unit],
        capabilities: &[Capability::StateRead],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::Subscribe,
        roles: &[Role::Unit],
        capabilities: &[Capability::StateRead],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::Unsubscribe,
        roles: &[Role::Unit],
        capabilities: &[Capability::StateRead],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::SetState,
        roles: &[Role::Unit],
        capabilities: &[Capability::StateWrite],
        surface: None,
    },
    OpPolicy {
        // An action's own capability is declared per action kind, because
        // "may act" is not one permission — see `crate::action`.
        //
        // Served to both: a unit acts within its grants, and the owner of the
        // daemon acts because it is the owner.
        kind: OpKind::Act,
        roles: &[Role::Unit, Role::Operator],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::EmitEvent,
        roles: &[Role::Unit],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        // Lifecycle belongs to whoever owns the daemon, and to nothing that
        // the daemon runs.
        kind: OpKind::RestartUnit,
        roles: &[Role::Operator],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        // Taking a unit's place hands out a token, so it is the operator's
        // alone — and grants them nothing new: they already own the state dir
        // the built binary is copied from, and the manifest still decides
        // what the adopted process may do.
        kind: OpKind::AdoptUnit,
        roles: &[Role::Operator],
        capabilities: &[],
        surface: None,
    },
];
