//! Operation authorization requirements.
//! Every served operation must have a policy row; unlisted operations are refused.

use omega_proto::omega::{Capability, SurfaceKind, invoke};

use crate::authorization::Role;

/// Every op in `wire.proto`. Exhaustive by construction: adding an op to the
/// schema fails to compile here until its kind is named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Storage,
    StorageSubscribe,
    StorageUnsubscribe,
    StorageInspect,
    CreateInstance,
    ChangePresentation,
    InspectInstances,
    AttachRenderer,
    ReportPresentation,
    Interact,
    SurfaceEvent,
    SurfaceLifecycle,
    GetState,
    SetState,
    Subscribe,
    Unsubscribe,
    Act,
    EmitEvent,
    PublishView,
    RestartPlugin,
    AdoptPlugin,
    ApplyShell,
    GetDeployment,
    CallCommand,
    RenderWidget,
    RemoveWidget,
}

impl OpKind {
    pub fn of(op: &invoke::Op) -> Self {
        match op {
            invoke::Op::Storage(_) => Self::Storage,
            invoke::Op::StorageSubscribe(_) => Self::StorageSubscribe,
            invoke::Op::StorageUnsubscribe(_) => Self::StorageUnsubscribe,
            invoke::Op::StorageInspect(_) => Self::StorageInspect,
            invoke::Op::CreateInstance(_) => Self::CreateInstance,
            invoke::Op::ChangePresentation(_) => Self::ChangePresentation,
            invoke::Op::InspectInstances(_) => Self::InspectInstances,
            invoke::Op::AttachRenderer(_) => Self::AttachRenderer,
            invoke::Op::ReportPresentation(_) => Self::ReportPresentation,
            invoke::Op::Interact(_) => Self::Interact,
            invoke::Op::SurfaceEvent(_) => Self::SurfaceEvent,
            invoke::Op::SurfaceLifecycle(_) => Self::SurfaceLifecycle,
            invoke::Op::GetState(_) => Self::GetState,
            invoke::Op::SetState(_) => Self::SetState,
            invoke::Op::Subscribe(_) => Self::Subscribe,
            invoke::Op::Unsubscribe(_) => Self::Unsubscribe,
            invoke::Op::Act(_) => Self::Act,
            invoke::Op::EmitEvent(_) => Self::EmitEvent,
            invoke::Op::PublishView(_) => Self::PublishView,
            invoke::Op::RestartPlugin(_) => Self::RestartPlugin,
            invoke::Op::AdoptPlugin(_) => Self::AdoptPlugin,
            invoke::Op::ApplyShell(_) => Self::ApplyShell,
            invoke::Op::GetDeployment(_) => Self::GetDeployment,
            invoke::Op::CallCommand(_) => Self::CallCommand,
            invoke::Op::RenderWidget(_) => Self::RenderWidget,
            invoke::Op::RemoveWidget(_) => Self::RemoveWidget,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Storage => "Storage",
            Self::StorageSubscribe => "StorageSubscribe",
            Self::StorageUnsubscribe => "StorageUnsubscribe",
            Self::StorageInspect => "StorageInspect",
            Self::CreateInstance => "CreateInstance",
            Self::ChangePresentation => "ChangePresentation",
            Self::InspectInstances => "InspectInstances",
            Self::AttachRenderer => "AttachRenderer",
            Self::ReportPresentation => "ReportPresentation",
            Self::Interact => "Interact",
            Self::SurfaceEvent => "SurfaceEvent",
            Self::SurfaceLifecycle => "SurfaceLifecycle",
            Self::GetState => "GetState",
            Self::SetState => "SetState",
            Self::Subscribe => "Subscribe",
            Self::Unsubscribe => "Unsubscribe",
            Self::Act => "Act",
            Self::EmitEvent => "EmitEvent",
            Self::PublishView => "PublishView",
            Self::RestartPlugin => "RestartPlugin",
            Self::AdoptPlugin => "AdoptPlugin",
            Self::ApplyShell => "ApplyShell",
            Self::GetDeployment => "GetDeployment",
            Self::CallCommand => "CallCommand",
            Self::RenderWidget => "RenderWidget",
            Self::RemoveWidget => "RemoveWidget",
        }
    }

    /// The surface an op targets, if it targets one. Exhaustive for the same
    /// reason: a new surface-scoped op must state that it is one.
    pub(super) fn target_surface(op: &invoke::Op) -> Option<&str> {
        match op {
            invoke::Op::PublishView(publish) => Some(&publish.surface_id),
            invoke::Op::RenderWidget(render) => Some(&render.surface_id),
            invoke::Op::RemoveWidget(remove) => Some(&remove.surface_id),
            invoke::Op::CreateInstance(_)
            | invoke::Op::ChangePresentation(_)
            | invoke::Op::InspectInstances(_)
            | invoke::Op::AttachRenderer(_)
            | invoke::Op::ReportPresentation(_)
            | invoke::Op::SurfaceLifecycle(_)
            | invoke::Op::SurfaceEvent(_)
            | invoke::Op::Interact(_)
            | invoke::Op::GetState(_)
            | invoke::Op::SetState(_)
            | invoke::Op::Subscribe(_)
            | invoke::Op::Unsubscribe(_)
            | invoke::Op::Act(_)
            | invoke::Op::EmitEvent(_)
            | invoke::Op::RestartPlugin(_)
            | invoke::Op::GetDeployment(_)
            | invoke::Op::ApplyShell(_)
            | invoke::Op::AdoptPlugin(_)
            | invoke::Op::CallCommand(_)
            | invoke::Op::Storage(_)
            | invoke::Op::StorageSubscribe(_)
            | invoke::Op::StorageUnsubscribe(_)
            | invoke::Op::StorageInspect(_) => None,
        }
    }
}

/// What an op requires of the peer that sent it.
pub(super) struct OpPolicy {
    pub(super) kind: OpKind,
    /// Who this op is served to. A plugin does not manage its neighbours, and
    /// an operator does not act as a plugin — but some ops belong to both.
    pub(super) roles: &'static [Role],
    /// Capabilities the plugin's manifest must grant.
    pub(super) capabilities: &'static [Capability],
    /// The kind the targeted surface must be declared as. Publishing to a
    /// surface a plugin did not declare is another plugin's business.
    pub(super) surface: Option<SurfaceKind>,
}

/// The ops this daemon serves, and what each demands. Deny by default:
/// anything absent is refused.
pub(super) const POLICY: &[OpPolicy] = &[
    OpPolicy {
        kind: OpKind::Storage,
        roles: &[Role::Plugin, Role::Operator],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::StorageSubscribe,
        roles: &[Role::Plugin],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::StorageUnsubscribe,
        roles: &[Role::Plugin],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::StorageInspect,
        roles: &[Role::Operator],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::CreateInstance,
        roles: &[Role::Operator],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::ChangePresentation,
        roles: &[Role::Operator, Role::Renderer, Role::Plugin],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::InspectInstances,
        roles: &[Role::Operator],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::AttachRenderer,
        roles: &[Role::Operator],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::ReportPresentation,
        roles: &[Role::Renderer],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::Interact,
        roles: &[Role::Renderer],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::GetDeployment,
        roles: &[Role::Operator],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::ApplyShell,
        roles: &[Role::Operator],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::PublishView,
        roles: &[Role::Plugin],
        capabilities: &[],
        surface: Some(SurfaceKind::Widget),
    },
    OpPolicy {
        kind: OpKind::GetState,
        roles: &[Role::Plugin],
        capabilities: &[Capability::StateRead],
        surface: None,
    },
    OpPolicy {
        // Subscriptions can only narrow the peer's authorized topic set.
        kind: OpKind::Subscribe,
        roles: &[Role::Plugin, Role::Operator, Role::Renderer],
        capabilities: &[Capability::StateRead],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::Unsubscribe,
        roles: &[Role::Plugin, Role::Operator, Role::Renderer],
        capabilities: &[Capability::StateRead],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::SetState,
        roles: &[Role::Plugin],
        capabilities: &[Capability::StateWrite],
        surface: None,
    },
    OpPolicy {
        // Action-specific capabilities are checked by the action dispatcher.
        // Operators act as the daemon owner; plugins remain within their grants.
        kind: OpKind::Act,
        roles: &[Role::Plugin, Role::Operator],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        kind: OpKind::EmitEvent,
        roles: &[Role::Plugin],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        // Lifecycle belongs to whoever owns the daemon, and to nothing that
        // the daemon runs.
        kind: OpKind::RestartPlugin,
        roles: &[Role::Operator],
        capabilities: &[],
        surface: None,
    },
    OpPolicy {
        // Only the operator may adopt a plugin; adoption retains its manifest grants.
        kind: OpKind::AdoptPlugin,
        roles: &[Role::Operator],
        capabilities: &[],
        surface: None,
    },
];
