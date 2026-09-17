use super::presentation_state::Visibility;
use super::{UnitTable, instance::Instance, session::SessionLink};
use crate::hub::SurfaceRef;
use crate::refusal::{Refusable, RefusableResult};
use omega_proto::instance::{InstanceKey, PresentationSpec, SingletonId};
use omega_proto::omega::{
    self, PresentationAction, PresentationState, Value, invoke, presentation, result,
};
use omega_proto::{Refusal, SurfaceId, UnitName};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug)]
pub(super) struct RendererLease {
    active: std::sync::Weak<std::sync::atomic::AtomicBool>,
    description: omega::AttachRenderer,
}

/// Construction facts captured together for one configured, ready instance.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledInstance {
    pub config: HashMap<String, Value>,
    pub presentation: PresentationSpec,
    pub anchor: Option<SurfaceRef>,
}

impl UnitTable {
    /// Snapshot configured instances and retained anchor addresses under the unit lock.
    /// Transient and still-starting instances do not participate in reconciliation.
    pub fn installed_presentations(&self) -> BTreeMap<SurfaceRef, InstalledInstance> {
        let records = self.lock();
        let views: BTreeMap<_, _> = self
            .inner
            .hub
            .view_snapshot()
            .into_iter()
            .map(|view| (view.instance.clone(), view.surface.clone()))
            .collect();
        let mut installed = BTreeMap::new();
        for record in records.values() {
            for instance in record.instances.values().filter(|instance| instance.ready) {
                let Some(address) = &instance.placement else {
                    continue;
                };
                let anchor = match &instance.presentation.wire().kind {
                    Some(presentation::Kind::Popup(popup)) => {
                        Self::instance_key(popup.anchor.as_ref())
                            .ok()
                            .and_then(|key| views.get(&key).cloned())
                    }
                    _ => None,
                };
                installed.insert(
                    address.clone(),
                    InstalledInstance {
                        config: instance.config.clone(),
                        presentation: instance.presentation.clone(),
                        anchor,
                    },
                );
            }
        }
        installed
    }

    pub fn instances(&self) -> BTreeMap<SurfaceRef, HashMap<String, Value>> {
        self.lock()
            .values()
            .flat_map(|record| record.instances.values())
            .filter(|instance| instance.ready && instance.placement.is_some())
            .map(|instance| {
                (
                    instance.placement.clone().expect("placed instance"),
                    instance.config.clone(),
                )
            })
            .collect()
    }

    pub fn inspect_instances(&self, unit: Option<&UnitName>) -> omega::InstanceList {
        let records = self.lock();
        omega::InstanceList {
            instances: records
                .values()
                .filter(|record| unit.is_none_or(|unit| unit == &record.name))
                .flat_map(|record| record.instances.values())
                .filter(|instance| instance.ready)
                .map(|instance| {
                    self.inner
                        .hub
                        .view(&instance.key)
                        .expect("ready instance has retained view")
                        .snapshot()
                })
                .collect(),
        }
    }

    pub async fn create_instance(
        &self,
        request: &omega::CreateInstance,
    ) -> Result<omega::InstanceSnapshot, Refusal> {
        let unit = request.unit.parse::<UnitName>().or_refuse()?;
        let surface = request.surface.parse::<SurfaceId>().or_refuse()?;
        let presentation = PresentationSpec::try_from(
            request
                .presentation
                .clone()
                .ok_or_else(|| Refusal::invalid("presentation is required"))?,
        )
        .or_refuse()?;
        if !matches!(
            presentation.wire().kind,
            Some(presentation::Kind::Window(_) | presentation::Kind::Overlay(_))
        ) {
            return Err(Refusal::invalid(
                "operator-created instances require a window or overlay",
            ));
        }
        if let Some(presentation::Kind::Window(window)) = &presentation.wire().kind
            && window.app_id != format!("org.omega.{unit}")
        {
            return Err(Refusal::invalid(
                "window application identity belongs to its plugin",
            ));
        }
        let singleton = if request.singleton.is_empty() {
            None
        } else {
            Some(request.singleton.parse::<SingletonId>().or_refuse()?)
        };
        self.create(
            &unit,
            surface,
            request.config.clone(),
            presentation,
            singleton,
            None,
        )
        .await
    }

    async fn create(
        &self,
        unit: &UnitName,
        surface: SurfaceId,
        config: HashMap<String, Value>,
        presentation: PresentationSpec,
        singleton: Option<SingletonId>,
        placement: Option<SurfaceRef>,
    ) -> Result<omega::InstanceSnapshot, Refusal> {
        let admission = 'admission: {
            let mut records = self.lock();
            let count: usize = records.values().map(|record| record.instances.len()).sum();
            let retained: usize = records
                .values()
                .flat_map(|record| record.instances.values())
                .map(Instance::config_bytes)
                .sum();
            let record = records
                .get_mut(unit)
                .ok_or_else(|| Refusal::invalid(format!("unknown unit {unit}")))?;
            let session = record
                .session
                .clone()
                .ok_or_else(|| Refusal::unavailable(format!("{unit} is not connected")))?;
            let manifest = record
                .manifest
                .as_ref()
                .ok_or_else(|| Refusal::precondition("unit has no manifest"))?;
            omega_document::DocumentValidation::surface(&manifest.manifest, surface.as_str())
                .or_refuse()?;
            if let Some(existing) = record.instances.values_mut().find(|held| {
                held.surface == surface
                    && (singleton.is_some() && held.singleton == singleton
                        || placement.is_some() && held.placement == placement)
            }) {
                if !existing.ready {
                    return Err(Refusal::unavailable("instance creation is in progress"));
                }
                if existing.config != config || existing.presentation != presentation {
                    return Err(Refusal::precondition(
                        "existing instance has different construction settings or presentation",
                    ));
                }
                break 'admission Admission::Existing(existing.key.clone());
            }
            if record.instances.len() >= 256 || count >= 4096 {
                return Err(Refusal::exhausted("instance capacity exhausted"));
            }
            let instance =
                Instance::new(surface, config, presentation, singleton, placement).or_refuse()?;
            if instance.config_bytes() > 128 * 1024
                || retained + instance.config_bytes() > 8 * 1024 * 1024
            {
                return Err(Refusal::exhausted(
                    "instance construction settings capacity exhausted",
                ));
            }
            record
                .instances
                .insert(instance.key.id.clone(), instance.clone());
            Admission::New(session, Box::new(instance))
        };
        let (session, instance) = match admission {
            Admission::Existing(key) => {
                self.transition_presentation(
                    &key,
                    None,
                    PresentationUpdate::Request(PresentationAction::Present),
                )
                .await?;
                return self
                    .inner
                    .hub
                    .view(&key)
                    .map(|view| view.snapshot())
                    .ok_or_else(|| Refusal::precondition("instance expired during presentation"));
            }
            Admission::New(session, instance) => (session, *instance),
        };
        let mut pending = Creation {
            units: self.clone(),
            unit: unit.clone(),
            key: instance.key.clone(),
            session: session.clone(),
            committed: false,
        };
        let answer = Self::request_on(
            &session,
            unit,
            invoke::Op::RenderWidget(omega::RenderWidget {
                surface_id: instance.surface.to_string(),
                instance: Some(instance.key.wire()),
                config: instance.config.clone(),
            }),
        )
        .await
        .or_refuse()?;
        let result::Outcome::View(view) = answer else {
            return Err(Refusal::invalid("RenderWidget requires a view result"));
        };
        let mut records = self.lock();
        let record = records
            .get_mut(unit)
            .ok_or_else(|| Refusal::unavailable("unit disappeared"))?;
        if !record
            .session
            .as_ref()
            .is_some_and(|current| current.requests.same_channel(&session.requests))
        {
            return Err(Refusal::precondition(
                "unit session changed during instance creation",
            ));
        }
        let held = record
            .instances
            .get_mut(&instance.key.id)
            .ok_or_else(|| Refusal::precondition("instance creation was cancelled"))?;
        let view = self
            .inner
            .hub
            .view(&held.key)
            .map_or(view, |published| published.view.clone());
        self.inner
            .hub
            .publish_view(held.update(unit, view))
            .or_refuse()?;
        held.ready = true;
        pending.committed = true;
        Ok(self
            .inner
            .hub
            .view(&held.key)
            .expect("published instance")
            .snapshot())
    }

    pub async fn configure_popup(
        &self,
        address: &SurfaceRef,
        config: HashMap<String, Value>,
        anchor: &SurfaceRef,
    ) -> Result<(), Refusal> {
        let key = self
            .lock()
            .get(&anchor.unit)
            .and_then(|record| {
                record
                    .instances
                    .values()
                    .find(|instance| instance.ready && instance.placement.as_ref() == Some(anchor))
                    .map(|instance| instance.key.wire())
            })
            .ok_or_else(|| Refusal::unavailable("popup anchor is not ready"))?;
        let presentation = PresentationSpec::try_from(omega::Presentation {
            kind: Some(presentation::Kind::Popup(omega::PopupPresentation {
                anchor: Some(key),
            })),
        })
        .or_refuse()?;
        self.configure_presentation(address, config, presentation)
            .await
    }

    pub async fn configure_presentation(
        &self,
        address: &SurfaceRef,
        config: HashMap<String, Value>,
        presentation: PresentationSpec,
    ) -> Result<(), Refusal> {
        if self.instances().contains_key(address) {
            self.remove_instance(address).await?;
        }
        self.create(
            &address.unit,
            address.surface.clone(),
            config,
            presentation,
            None,
            Some(address.clone()),
        )
        .await?;
        Ok(())
    }

    pub async fn configure_instance(
        &self,
        address: &SurfaceRef,
        config: HashMap<String, Value>,
    ) -> Result<(), Refusal> {
        if self
            .instances()
            .get(address)
            .is_some_and(|held| held != &config)
        {
            self.remove_instance(address).await?;
        }
        let placement = address
            .module
            .as_ref()
            .ok_or_else(|| Refusal::invalid("bar instance requires a placement"))?;
        let presentation = PresentationSpec::try_from(omega::Presentation {
            kind: Some(presentation::Kind::Embedded(omega::EmbeddedPresentation {
                placement: placement.to_string(),
            })),
        })
        .or_refuse()?;
        self.create(
            &address.unit,
            address.surface.clone(),
            config,
            presentation,
            None,
            Some(address.clone()),
        )
        .await?;
        Ok(())
    }

    pub async fn remove_instance(&self, address: &SurfaceRef) -> Result<(), Refusal> {
        let key = self.lock().get(&address.unit).and_then(|record| {
            record
                .instances
                .values()
                .find(|instance| instance.placement.as_ref() == Some(address))
                .map(|instance| instance.key.clone())
        });
        if let Some(key) = key {
            self.destroy_instance(&key).await?;
        }
        Ok(())
    }

    pub fn publish_instance(
        &self,
        unit: &UnitName,
        publish: &omega::PublishView,
    ) -> Result<(), Refusal> {
        let key = Self::instance_key(publish.instance.as_ref())?;
        let records = self.lock();
        let instance = records
            .get(unit)
            .and_then(|record| record.instances.get(&key.id))
            .filter(|instance| instance.key == key)
            .ok_or_else(|| Refusal::precondition("unknown or expired instance"))?;
        if instance.surface.as_str() != publish.surface_id {
            return Err(Refusal::denied("instance belongs to another surface"));
        }
        self.inner
            .hub
            .publish_view(
                instance.update(
                    unit,
                    publish
                        .view
                        .clone()
                        .ok_or_else(|| Refusal::invalid("PublishView requires a view"))?,
                ),
            )
            .or_refuse()
    }

    pub(crate) fn owns_instance(&self, unit: &UnitName, key: &InstanceKey) -> bool {
        self.lock()
            .get(unit)
            .and_then(|record| record.instances.get(&key.id))
            .is_some_and(|instance| instance.key == *key && instance.ready)
    }

    pub fn instance_key(value: Option<&omega::InstanceRef>) -> Result<InstanceKey, Refusal> {
        InstanceKey::try_from(
            value.ok_or_else(|| Refusal::invalid("instance identity is required"))?,
        )
        .or_refuse()
    }

    pub(crate) async fn change_presentation(
        &self,
        request: &omega::ChangePresentation,
        permit: Option<&crate::attachment::InstancePermit>,
    ) -> Result<(), Refusal> {
        let key = Self::instance_key(request.instance.as_ref())?;
        let action = PresentationAction::try_from(request.action)
            .map_err(|_| Refusal::invalid("unknown presentation action"))?;
        if action == PresentationAction::Destroy {
            return self.destroy_instance(&key).await;
        }
        self.transition_presentation(&key, permit, PresentationUpdate::Request(action))
            .await
    }

    pub(crate) async fn report_presentation(
        &self,
        request: &omega::ReportPresentation,
        permit: &crate::attachment::InstancePermit,
    ) -> Result<(), Refusal> {
        let key = Self::instance_key(request.instance.as_ref())?;
        let observed = Visibility::observed(request.observed)?;
        self.transition_presentation(&key, Some(permit), PresentationUpdate::Observed(observed))
            .await
    }

    fn lifecycle_gate(
        &self,
        key: &InstanceKey,
    ) -> Result<std::sync::Arc<tokio::sync::Mutex<()>>, Refusal> {
        self.lock()
            .values()
            .find_map(|record| {
                record
                    .instances
                    .get(&key.id)
                    .filter(|instance| instance.key == *key && instance.ready)
                    .map(|instance| instance.lifecycle.clone())
            })
            .ok_or_else(|| Refusal::precondition("unknown or expired instance"))
    }

    async fn transition_presentation(
        &self,
        key: &InstanceKey,
        permit: Option<&crate::attachment::InstancePermit>,
        update: PresentationUpdate,
    ) -> Result<(), Refusal> {
        // Intent publication and its acknowledgement must retain their order per instance.
        let _lifecycle = self.lifecycle_gate(key)?.lock_owned().await;
        let notification = {
            let mut records = self.lock();
            if let Some(permit) = permit {
                permit.validate()?;
            }
            let record = records
                .values_mut()
                .find(|record| {
                    record
                        .instances
                        .get(&key.id)
                        .is_some_and(|instance| instance.key == *key && instance.ready)
                })
                .ok_or_else(|| Refusal::precondition("unknown or expired instance"))?;
            let instance = record
                .instances
                .get_mut(&key.id)
                .expect("resolved instance");
            let mut next = instance.clone();
            next.state = match update {
                PresentationUpdate::Request(action) => {
                    next.state.request(Visibility::requested(action)?)
                }
                PresentationUpdate::Observed(state) => next.state.report(state),
            };
            let changed = next.state.requested() != instance.state.requested();
            let view = self.inner.hub.view(key).expect("ready instance");
            self.inner
                .hub
                .publish_view(next.update(&record.name, view.view.clone()))
                .or_refuse()?;
            *instance = next;
            if changed {
                record
                    .session
                    .clone()
                    .map(|session| (record.name.clone(), session, instance.state.requested()))
            } else {
                None
            }
        };
        if let Some((unit, session, state)) = notification {
            let delivery = LifecycleDelivery(Some(session.stop.clone()));
            let outcome = Self::request_on(
                &session,
                &unit,
                invoke::Op::SurfaceLifecycle(omega::SurfaceLifecycle {
                    instance: Some(key.wire()),
                    state: state.wire(),
                }),
            )
            .await;
            match outcome {
                Ok(result::Outcome::Ok(_)) => delivery.acknowledged(),
                Ok(_) => {
                    return Err(Refusal::precondition(
                        "surface lifecycle was not acknowledged",
                    ));
                }
                Err(error) => return Err(error.refusal()),
            }
        }
        Ok(())
    }

    async fn destroy_instance(&self, key: &InstanceKey) -> Result<(), Refusal> {
        let _lifecycle = self.lifecycle_gate(key)?.lock_owned().await;
        let (unit, instance, session) = {
            let mut records = self.lock();
            let record = records
                .values_mut()
                .find(|record| {
                    record
                        .instances
                        .get(&key.id)
                        .is_some_and(|instance| &instance.key == key && instance.ready)
                })
                .ok_or_else(|| Refusal::precondition("unknown or expired instance"))?;
            let instance = record.instances.remove(&key.id).expect("resolved instance");
            self.inner.hub.drop_instance(key);
            (record.name.clone(), instance, record.session.clone())
        };
        if let Some(session) = session {
            let mut pending = Creation {
                units: self.clone(),
                unit: unit.clone(),
                key: key.clone(),
                session: session.clone(),
                committed: false,
            };
            let answer = Self::request_on(
                &session,
                &unit,
                invoke::Op::RemoveWidget(omega::RemoveWidget {
                    surface_id: instance.surface.to_string(),
                    instance: Some(key.wire()),
                }),
            )
            .await
            .or_refuse()?;
            if !matches!(answer, result::Outcome::Ok(_)) {
                return Err(Refusal::invalid("RemoveWidget requires an ok result"));
            }
            pending.committed = true;
        }
        Ok(())
    }
}

/// Cancellation after a request was queued cannot leave untracked SDK instances alive.
struct Creation {
    units: UnitTable,
    unit: UnitName,
    key: InstanceKey,
    session: SessionLink,
    committed: bool,
}
impl Drop for Creation {
    fn drop(&mut self) {
        if !self.committed {
            if let Some(record) = self.units.lock().get_mut(&self.unit) {
                record.instances.remove(&self.key.id);
                self.units.inner.hub.drop_instance(&self.key);
            }
            self.session.stop.trigger();
        }
    }
}

impl UnitTable {
    pub(crate) async fn interact(
        &self,
        permit: &crate::attachment::InstancePermit,
        event: &omega::Interact,
    ) -> Result<omega_proto::CommandAnswer, Refusal> {
        let key = &permit.key;
        let (unit, session, call) = {
            let records = self.lock();
            permit.validate()?;
            let record = records
                .values()
                .find(|record| {
                    record
                        .instances
                        .get(&key.id)
                        .is_some_and(|instance| instance.ready && &instance.key == key)
                })
                .ok_or_else(|| Refusal::precondition("unknown or expired instance"))?;
            let view = self
                .inner
                .hub
                .view(key)
                .ok_or_else(|| Refusal::precondition("instance has no view"))?;
            if view.view.revision != event.revision {
                return Err(Refusal::precondition(
                    "interaction refers to an obsolete view",
                ));
            }
            if view.requested != PresentationState::Visible as i32 {
                return Err(Refusal::precondition("presentation is not visible"));
            }
            let op = match omega_proto::Interaction::resolve(&view.view, &event.node, &event.event)?
            {
                omega_proto::Interaction::Local(binding) => {
                    invoke::Op::SurfaceEvent(omega::SurfaceEvent {
                        instance: Some(key.wire()),
                        binding: binding.get(),
                        value: event.value.clone(),
                    })
                }
                omega_proto::Interaction::Command { command, args } => {
                    let manifest = record
                        .manifest
                        .as_ref()
                        .ok_or_else(|| Refusal::precondition("unit has no manifest"))?;
                    if !manifest
                        .manifest
                        .commands
                        .iter()
                        .any(|declared| declared.id == command)
                    {
                        return Err(Refusal::denied("binding targets an undeclared command"));
                    }
                    let mut args = args.to_vec();
                    if let Some(value) = &event.value {
                        args.push(value.clone());
                    }
                    invoke::Op::CallCommand(omega::CallCommand {
                        command: command.to_owned(),
                        args,
                    })
                }
            };
            (
                record.name.clone(),
                record
                    .session
                    .clone()
                    .ok_or_else(|| Refusal::unavailable("unit disconnected"))?,
                op,
            )
        };
        omega_proto::CommandAnswer::try_from(
            Self::request_on(&session, &unit, call).await.or_refuse()?,
        )
    }
}

impl UnitTable {
    pub(crate) fn renderer_placements(&self) -> Vec<omega::PlacementAttachment> {
        self.lock()
            .values()
            .flat_map(|record| record.instances.values())
            .filter(|instance| {
                instance.ready
                    && matches!(
                        instance.presentation.wire().kind,
                        Some(presentation::Kind::Embedded(_) | presentation::Kind::Popup(_))
                    )
            })
            .map(|instance| {
                let placement = instance
                    .placement
                    .as_ref()
                    .expect("placed presentation has an address");
                omega::PlacementAttachment {
                    unit: placement.unit.to_string(),
                    surface: placement.surface.to_string(),
                    placement: placement
                        .module
                        .as_ref()
                        .expect("placed presentation has a module")
                        .to_string(),
                }
            })
            .collect()
    }

    pub(crate) fn renderer_statuses(&self) -> Vec<omega::AttachRenderer> {
        self.inner
            .renderers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|lease| {
                lease
                    .active
                    .upgrade()
                    .is_some_and(|active| active.load(std::sync::atomic::Ordering::Acquire))
            })
            .map(|lease| lease.description.clone())
            .collect()
    }

    pub(crate) fn claim_renderer(&self, attachment: &crate::attachment::Attachment) {
        let _records = self.lock();
        let mut slots = self
            .inner
            .renderers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        slots.retain(|_, lease| lease.active.strong_count() != 0);
        if let Some(previous) = slots.insert(
            attachment.scope.clone(),
            RendererLease {
                active: std::sync::Arc::downgrade(&attachment.active),
                description: attachment.description(),
            },
        ) && let Some(previous) = previous.active.upgrade()
        {
            previous.store(false, std::sync::atomic::Ordering::Release);
        }
    }

    pub(crate) fn renderer_disconnected(
        &self,
        keys: &[InstanceKey],
        active: &std::sync::atomic::AtomicBool,
    ) {
        let mut records = self.lock();
        if !active.swap(false, std::sync::atomic::Ordering::AcqRel) {
            return;
        }
        for key in keys {
            for record in records.values_mut() {
                if let Some(instance) = record
                    .instances
                    .get_mut(&key.id)
                    .filter(|instance| &instance.key == key && instance.ready)
                {
                    instance.state = instance.state.disconnected();
                    if let Some(view) = self.inner.hub.view(key)
                        && let Err(error) = self
                            .inner
                            .hub
                            .publish_view(instance.update(&record.name, view.view.clone()))
                    {
                        tracing::error!(%error, "cannot clear disconnected renderer observation");
                    }
                }
            }
        }
    }
}

enum PresentationUpdate {
    Request(PresentationAction),
    Observed(Visibility),
}

enum Admission {
    Existing(InstanceKey),
    New(SessionLink, Box<Instance>),
}

// Cancelling the request must not leave daemon intent ahead of plugin lifecycle.
struct LifecycleDelivery(Option<crate::Shutdown>);
impl LifecycleDelivery {
    fn acknowledged(mut self) {
        self.0.take();
    }
}
impl Drop for LifecycleDelivery {
    fn drop(&mut self) {
        if let Some(stop) = self.0.take() {
            stop.trigger();
        }
    }
}
