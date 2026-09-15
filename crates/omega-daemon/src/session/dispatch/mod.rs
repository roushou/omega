//! Dispatch admitted operations through the shared policy table.
//! An operation without a policy row is refused as unimplemented.

use omega_proto::omega::{
    Empty, Frame, Invoke, StatePatch, StateTopic, Value, invoke, result, state_topic,
};
use omega_proto::{Address, CommandAnswer, Refusal};
use omega_proto::{SurfaceId, UnitName};

use crate::action::Actions;
use crate::attachment;
use crate::broker::Brokerage;
use crate::refusal::RefusableResult;

use crate::authorization::{Grants, Role};
use crate::hub::Hub;
use crate::session::admission::Peer;
use crate::session::subscriptions::Subscriptions;
use crate::supervisor::Supervisor;
use crate::units::UnitTable;

mod adoption;
mod policy;

use adoption::Adoptions;
use policy::{OpPolicy, POLICY};

pub use policy::OpKind;

/// What a served op answers with, on the stream that asked.
#[derive(Debug)]
pub enum Response {
    /// The op succeeded and carries nothing back.
    Ok,
    Instances(omega_proto::omega::InstanceList),
    /// Topic values, for `GetState`.
    State(StatePatch),
    /// Whatever a unit answered with, for an op that asked it something.
    Value(Value),
    Deployment(omega_proto::omega::DeploymentStatus),
}

impl From<CommandAnswer> for Response {
    fn from(answer: CommandAnswer) -> Self {
        match answer {
            CommandAnswer::Acknowledged => Self::Ok,
            CommandAnswer::Value(value) => Self::Value(value),
        }
    }
}

impl Response {
    pub fn frame(self, stream_id: u64) -> Frame {
        let outcome = match self {
            Self::Instances(instances) => result::Outcome::Instances(instances),
            Self::Ok => result::Outcome::Ok(Empty {}),
            Self::State(patch) => result::Outcome::State(patch),
            Self::Value(value) => result::Outcome::Value(value),
            Self::Deployment(status) => result::Outcome::Deployment(status),
        };
        Frame::reply(stream_id, outcome)
    }
}

/// Per-connection invocation authorization and dispatch. Dropping it releases adoptions.
#[derive(Debug)]
pub struct Dispatcher {
    attachment: Option<attachment::RendererAttachment>,
    hub: Hub,
    supervisor: Supervisor,
    units: UnitTable,
    brokers: Brokerage,
    adopted: Adoptions,
    layout: Option<omega_host::Layout>,
    deployment: crate::reconcile::deployment::Deployment,
}

impl Dispatcher {
    pub fn new(hub: Hub, supervisor: Supervisor, units: UnitTable, brokers: Brokerage) -> Self {
        Self {
            attachment: None,
            hub,
            layout: None,
            deployment: Default::default(),
            adopted: Adoptions::new(supervisor.clone()),
            supervisor,
            units,
            brokers,
        }
    }

    pub(crate) fn with_attachment(mut self, attachment: attachment::RendererAttachment) -> Self {
        self.attachment = Some(attachment);
        self
    }

    pub fn with_deployment(mut self, deployment: crate::reconcile::deployment::Deployment) -> Self {
        self.deployment = deployment;
        self
    }

    pub fn with_layout(mut self, layout: Option<omega_host::Layout>) -> Self {
        self.layout = layout;
        self
    }

    /// Authorize, then serve. Every refusal is returned, never swallowed.
    pub async fn invoke(
        &self,
        peer: &Peer,
        subscriptions: &mut Subscriptions,
        invoke: &Invoke,
    ) -> Result<Response, Refusal> {
        let op = invoke
            .op
            .as_ref()
            .ok_or_else(|| Refusal::invalid("invoke carries no op"))?;

        let kind = OpKind::of(op);
        let policy = POLICY
            .iter()
            .find(|policy| policy.kind == kind)
            .ok_or_else(|| {
                Refusal::unimplemented(format!("{} is not served by this daemon", kind.name()))
            })?;

        let role = if self.attachment.as_ref().is_some_and(|attachment| {
            attachment
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
        }) {
            Role::Renderer
        } else {
            peer.role()
        };
        if !policy.roles.contains(&role) {
            return Err(Refusal::denied(format!(
                "{} is not served to {}s",
                kind.name(),
                match role {
                    Role::Renderer => "renderer",
                    Role::Unit => "unit",
                    Role::Operator => "operator",
                }
            )));
        }

        // Operator authorization is role-based; units additionally require manifest capabilities.
        if peer.role() == Role::Unit {
            Self::authorize(policy, peer.grants(), op)?;
        }

        self.serve(peer, subscriptions, op).await
    }

    fn authorize_instance(
        &self,
        instance: Option<&omega_proto::omega::InstanceRef>,
    ) -> Result<attachment::InstancePermit, Refusal> {
        let key = UnitTable::instance_key(instance)?;
        let slot = self
            .attachment
            .as_ref()
            .ok_or_else(|| Refusal::denied("renderer attachment required"))?;
        let attachment = slot.lock().unwrap_or_else(|e| e.into_inner());
        attachment
            .as_ref()
            .ok_or_else(|| Refusal::denied("renderer attachment required"))?
            .authorize(&self.hub, &key)?;
        Ok(attachment::InstancePermit::new(
            attachment.as_ref().expect("authorized attachment"),
            key,
        ))
    }

    fn authorize(policy: &OpPolicy, grants: &Grants, op: &invoke::Op) -> Result<(), Refusal> {
        for capability in policy.capabilities {
            if !grants.holds(*capability) {
                return Err(Refusal::denied(format!(
                    "{} requires {}",
                    policy.kind.name(),
                    capability.as_str_name()
                )));
            }
        }

        let Some(required) = policy.surface else {
            return Ok(());
        };
        let named = OpKind::target_surface(op)
            .ok_or_else(|| Refusal::invalid(format!("{} names no surface", policy.kind.name())))?;
        // An id that cannot be a surface id names no surface of anyone's.
        let target = SurfaceId::parse(named).or_refuse()?;

        match grants.surface(&target) {
            Some(kind) if kind == required => Ok(()),
            Some(kind) => Err(Refusal::denied(format!(
                "surface {target:?} is declared as {}, not {}",
                kind.as_str_name(),
                required.as_str_name()
            ))),
            None => Err(Refusal::denied(format!(
                "surface {target:?} is not declared by this unit"
            ))),
        }
    }

    async fn serve(
        &self,
        peer: &Peer,
        subscriptions: &mut Subscriptions,
        op: &invoke::Op,
    ) -> Result<Response, Refusal> {
        match op {
            invoke::Op::CreateInstance(create) => {
                Ok(Response::Instances(omega_proto::omega::InstanceList {
                    instances: vec![self.units.create_instance(create).await?],
                }))
            }
            invoke::Op::ChangePresentation(change) => {
                if peer.role() == Role::Unit {
                    let key = UnitTable::instance_key(change.instance.as_ref())?;
                    if !peer
                        .unit_name()
                        .is_some_and(|unit| self.units.owns_instance(unit, &key))
                    {
                        return Err(Refusal::denied(
                            "a plugin may only change its own instances",
                        ));
                    }
                    if !matches!(
                        omega_proto::omega::PresentationAction::try_from(change.action),
                        Ok(omega_proto::omega::PresentationAction::Hide
                            | omega_proto::omega::PresentationAction::Close)
                    ) {
                        return Err(Refusal::denied(
                            "plugins may only hide or close their instances",
                        ));
                    }
                }
                let permit =
                    if self.attachment.as_ref().is_some_and(|slot| {
                        slot.lock().unwrap_or_else(|e| e.into_inner()).is_some()
                    }) {
                        if change.action == omega_proto::omega::PresentationAction::Destroy as i32 {
                            return Err(Refusal::denied("renderers cannot destroy instances"));
                        }
                        Some(self.authorize_instance(change.instance.as_ref())?)
                    } else {
                        None
                    };
                self.units
                    .change_presentation(change, permit.as_ref())
                    .await?;
                Ok(Response::Ok)
            }
            invoke::Op::InspectInstances(inspect) => {
                let unit = if inspect.unit.is_empty() {
                    None
                } else {
                    Some(UnitName::parse(&inspect.unit).or_refuse()?)
                };
                Ok(Response::Instances(
                    self.units.inspect_instances(unit.as_ref()),
                ))
            }
            invoke::Op::AttachRenderer(request) => {
                let slot = self.attachment.as_ref().ok_or_else(|| {
                    Refusal::precondition("renderer attachment requires the observation socket")
                })?;
                let attachment = attachment::Attachment::parse(request)?;
                if self.units.manifest(attachment.unit()).is_none() {
                    return Err(Refusal::invalid("unknown renderer unit"));
                }
                let mut held = slot.lock().unwrap_or_else(|e| e.into_inner());
                if held.is_some() {
                    return Err(Refusal::precondition("renderer is already attached"));
                }
                let mut instances = Vec::new();
                for view in self.hub.view_snapshot() {
                    if attachment.accepts(&view) {
                        attachment.validate(&view)?;
                        let mut metadata = view.snapshot();
                        metadata.view = None;
                        instances.push(metadata);
                    }
                }
                self.units.claim_renderer(&attachment);
                *held = Some(attachment);
                Ok(Response::Instances(omega_proto::omega::InstanceList {
                    instances,
                }))
            }
            invoke::Op::ReportPresentation(report) => {
                let permit = self.authorize_instance(report.instance.as_ref())?;
                self.units.report_presentation(report, &permit).await?;
                Ok(Response::Ok)
            }
            invoke::Op::Interact(interact) => {
                let key = self.authorize_instance(interact.instance.as_ref())?;
                self.units
                    .interact(&key, interact)
                    .await
                    .map(Response::from)
            }
            invoke::Op::PublishView(publish) => {
                let unit = peer
                    .unit_name()
                    .ok_or_else(|| Refusal::denied("only units publish views"))?;
                self.units.publish_instance(unit, publish)?;
                Ok(Response::Ok)
            }

            invoke::Op::GetState(get) => {
                // Reading is bounded by the subscription, so a `GetState` can
                // never reach past what the manifest declared.
                Ok(Response::State(
                    subscriptions.read(self.hub.snapshot(), &get.topics)?,
                ))
            }

            invoke::Op::Subscribe(subscribe) => {
                subscriptions.select(subscribe)?;
                Ok(Response::Ok)
            }

            invoke::Op::Unsubscribe(unsubscribe) => {
                subscriptions.unsubscribe(&unsubscribe.topics);
                subscriptions.unsubscribe_events(&unsubscribe.events);
                Ok(Response::Ok)
            }

            invoke::Op::Act(act) => {
                let action = act
                    .action
                    .as_ref()
                    .and_then(|action| action.kind.as_ref())
                    .ok_or_else(|| Refusal::invalid("Act carries no action"))?;

                // Check the capability for this action kind.
                if peer.role() == Role::Unit {
                    Actions::authorize(action, peer.grants())?;
                }
                Actions::new(self.units.clone(), self.brokers.clone())
                    .perform(action)
                    .await
                    .map(Response::from)
            }

            invoke::Op::EmitEvent(emit) => {
                let unit = peer
                    .unit_name()
                    .ok_or_else(|| Refusal::denied("only units emit events"))?;
                let event = emit
                    .event
                    .as_ref()
                    .ok_or_else(|| Refusal::invalid("EmitEvent carries no event"))?;

                // The unit field is the daemon's, not the frame's: an event
                // cannot claim to come from another unit.
                self.hub
                    .publish_custom_event(unit.as_str(), &event.name, event.payload.clone())
                    .or_refuse()?;
                Ok(Response::Ok)
            }

            invoke::Op::SetState(set) => {
                let unit = peer
                    .unit_name()
                    .ok_or_else(|| Refusal::denied("only units own a keyspace"))?;

                // Units may write only their own record keyspace.
                let topic = Address::parse(&set.topic).or_refuse()?;
                if topic.owner() != Some(unit.as_str()) {
                    return Err(Refusal::denied(format!(
                        "{topic} is not in {unit}'s keyspace"
                    )));
                }

                let value = set
                    .value
                    .clone()
                    .ok_or_else(|| Refusal::invalid("SetState carries no value"))?;

                self.hub
                    .publish_state(StatePatch {
                        topics: vec![StateTopic {
                            topic: topic.to_string(),
                            revision: 0, // the Hub assigns the real revision
                            value: Some(state_topic::Value::Generic(value)),
                        }],
                    })
                    .or_refuse()?;
                Ok(Response::Ok)
            }

            invoke::Op::GetDeployment(_) => {
                let mut status = self.deployment.snapshot();
                status.units = self.units.statuses();
                status.renderers = self.units.renderer_statuses();
                status.renderer_placements = self.units.renderer_placements();
                Ok(Response::Deployment(status))
            }
            invoke::Op::ApplyShell(apply) => {
                let layout = self
                    .layout
                    .as_ref()
                    .ok_or_else(|| Refusal::precondition("shell application is not configured"))?
                    .clone();
                let overwrite = apply.overwrite;
                let deployment = self.deployment.clone();
                let _handover = self.supervisor.handover().await;
                tokio::task::spawn_blocking(move || {
                    crate::reconcile::shell::ShellApplication::apply(
                        &layout,
                        overwrite,
                        &deployment,
                    )
                })
                .await
                .map_err(|error| Refusal::unavailable(error.to_string()))?
                .or_refuse()?;
                Ok(Response::Ok)
            }
            invoke::Op::AdoptUnit(adopt) => {
                let name = UnitName::parse(adopt.unit.clone()).or_refuse()?;

                let token = self.supervisor.adopt_unit(&name).await?;
                self.adopted.taken(name.clone(), token.clone());
                tracing::info!(unit = %name, "adopted for development");

                Ok(Response::Value(Value {
                    kind: Some(omega_proto::omega::value::Kind::StringValue(
                        token.as_str().to_string(),
                    )),
                }))
            }

            invoke::Op::RestartUnit(restart) => {
                let name = UnitName::parse(restart.unit.clone()).or_refuse()?;

                // Restart preserves the document's desired running state.
                if !self.supervisor.restart(&name) {
                    return Err(Refusal::invalid(format!("{name} is not running")));
                }
                Ok(Response::Ok)
            }

            other => Err(Refusal::unimplemented(format!(
                "{} is not served by this daemon",
                OpKind::of(other).name()
            ))),
        }
    }
}

impl Drop for Dispatcher {
    fn drop(&mut self) {
        if let Some(slot) = &self.attachment {
            let held = slot.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(attachment) = held.as_ref() {
                let keys: Vec<_> = self
                    .hub
                    .view_snapshot()
                    .iter()
                    .filter(|view| attachment.accepts(view))
                    .map(|view| view.instance.clone())
                    .collect();
                self.units.renderer_disconnected(&keys, &attachment.active);
            }
        }
    }
}
