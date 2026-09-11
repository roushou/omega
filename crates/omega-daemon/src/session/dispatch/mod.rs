//! What an admitted peer may ask for.
//!
//! Every op the wire defines is listed in [`OpKind`], and the policy for the
//! ones this daemon serves is declared in one table. An op with no row is
//! refused as unimplemented, so a handler can never become reachable without
//! a policy: authorization is not something a call site can forget.

use omega_proto::omega::{
    Empty, Frame, Invoke, StatePatch, StateTopic, Value, invoke, result, state_topic,
};
use omega_proto::{Address, Refusal};
use omega_proto::{ModuleId, SurfaceId, UnitName};

use crate::action::Actions;
use crate::broker::Brokerage;
use crate::refusal::RefusableResult;

use crate::hub::{Hub, SurfaceRef, ViewUpdate};
use crate::session::admission::{Grants, Peer, Role};
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
    /// Topic values, for `GetState`.
    State(StatePatch),
    /// Whatever a unit answered with, for an op that asked it something.
    Value(Value),
}

impl Response {
    pub fn frame(self, stream_id: u64) -> Frame {
        let outcome = match self {
            Self::Ok => result::Outcome::Ok(Empty {}),
            Self::State(patch) => result::Outcome::State(patch),
            Self::Value(value) => result::Outcome::Value(value),
        };
        Frame::reply(stream_id, outcome)
    }
}

/// Applies the policy table to one peer's invocations.
///
/// One per connection, and it outlives no connection: the units this peer
/// adopted are given back when it drops.
#[derive(Debug)]
pub struct Dispatcher {
    hub: Hub,
    supervisor: Supervisor,
    units: UnitTable,
    brokers: Brokerage,
    adopted: Adoptions,
    layout: Option<omega_host::Layout>,
}

impl Dispatcher {
    pub fn new(hub: Hub, supervisor: Supervisor, units: UnitTable, brokers: Brokerage) -> Self {
        Self {
            hub,
            layout: None,
            adopted: Adoptions::new(supervisor.clone()),
            supervisor,
            units,
            brokers,
        }
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

        if !policy.roles.contains(&peer.role()) {
            return Err(Refusal::denied(format!(
                "{} is not served to {}s",
                kind.name(),
                match peer.role() {
                    Role::Unit => "unit",
                    Role::Operator => "operator",
                }
            )));
        }

        // Capabilities bound what the daemon's own children may do. The
        // operator is not a child of the daemon; the role check above is its
        // authorization, and it holds no manifest to check against anyway.
        if peer.role() == Role::Unit {
            Self::authorize(policy, peer.grants(), op)?;
        }

        self.serve(peer, subscriptions, op).await
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
            invoke::Op::PublishView(publish) => {
                let view = publish
                    .view
                    .as_ref()
                    .ok_or_else(|| Refusal::invalid("PublishView carries no view"))?;

                // Authorization proved the peer is a unit that declared this
                // surface, so the reference is built from the daemon's own
                // identity for it — never from the frame.
                let unit = peer
                    .unit_name()
                    .ok_or_else(|| Refusal::denied("only units publish views"))?;

                let surface = SurfaceId::parse(publish.surface_id.clone()).or_refuse()?;

                // The module is the unit's own sub-namespace within a
                // surface it owns, so it needs no separate authorization —
                // whatever it calls an instance, it is still its instance.
                let module = if publish.module_id.is_empty() {
                    None
                } else {
                    Some(ModuleId::parse(publish.module_id.clone()).or_refuse()?)
                };

                self.hub
                    .publish_view(ViewUpdate {
                        surface: SurfaceRef {
                            unit: unit.clone(),
                            surface,
                            module,
                        },
                        view: view.clone(),
                    })
                    .or_refuse()?;
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

                // What an action costs is declared per action, not per op: a
                // unit granted the shell escape hatch has not thereby been
                // granted the power button.
                if peer.role() == Role::Unit {
                    Actions::authorize(action, peer.grants())?;
                }
                Actions::new(self.units.clone(), self.brokers.clone())
                    .perform(action)
                    .await
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

                // A unit writes its own keyspace and nothing else: system
                // topics belong to the daemon, and another unit's keyspace to
                // that unit, whatever capability the writer holds.
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

            invoke::Op::ApplyShell(apply) => {
                let layout = self
                    .layout
                    .as_ref()
                    .ok_or_else(|| Refusal::precondition("shell application is not configured"))?
                    .clone();
                let overwrite = apply.overwrite;
                let _handover = self.supervisor.handover().await;
                tokio::task::spawn_blocking(move || {
                    crate::reconcile::shell::ShellApplication::apply(&layout, overwrite)
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

                // Restarting is not a change of intent: the document still
                // says this unit should run, so the supervisor cycles the
                // process rather than stopping the unit.
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
