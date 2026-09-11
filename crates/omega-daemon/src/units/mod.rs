//! The units this daemon knows about.
//!
//! One table, one lock, one answer to "what do we know about unit X" — its
//! manifest, its lifecycle, the token it proves itself with, the handles that
//! stop and cycle it, and the session it is reachable through.
//!
//! The `units` state topic is a projection of this table, published whenever
//! it changes. Nothing else keeps a parallel record.

pub mod lifecycle;
pub mod record;
pub mod session;
pub mod token;

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::{mpsc, oneshot};

use omega_proto::UnitName;
use omega_proto::omega::{
    StatePatch, StateTopic, UnitStatus, UnitsState, Value, invoke, result, state_topic,
};
use omega_proto::{Refusal, SystemTopic};

use crate::Shutdown;
use crate::hub::Hub;
use crate::manifest::{ManifestStore, UnitManifest};

pub use lifecycle::{Lifecycle, Transition};
pub use omega_proto::DaemonStreams;
pub use record::{UnitControl, UnitRecord};
pub use session::{Request, RequestError, SessionGuard};
pub use token::UnitToken;

#[derive(Debug, Clone)]
pub struct UnitTable {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    units: Mutex<BTreeMap<UnitName, UnitRecord>>,
    request_bytes: Arc<tokio::sync::Semaphore>,
    hub: Hub,
    /// Announces a unit becoming reachable. A unit is started before it can
    /// be asked for anything, so whoever wants to ask has to be told when it
    /// arrives.
    connected: mpsc::Sender<UnitName>,
}

impl UnitTable {
    const REQUEST_BYTES: usize = 8 * 1024 * 1024;
    /// A table, and the stream of units connecting to it. Only the daemon
    /// holds the receiving end.
    pub fn new(hub: Hub) -> (Self, mpsc::Receiver<UnitName>) {
        let (connected, arrivals) = mpsc::channel(16);
        (
            Self {
                inner: Arc::new(Inner {
                    units: Mutex::new(BTreeMap::new()),
                    request_bytes: Arc::new(tokio::sync::Semaphore::new(Self::REQUEST_BYTES)),
                    hub,
                    connected,
                }),
            },
            arrivals,
        )
    }

    /// A table nothing listens to, for tests and in-process cores.
    pub fn detached(hub: Hub) -> Self {
        Self::new(hub).0
    }

    // ---- the build ----

    /// Adopt the manifests of a build. Units the build no longer contains
    /// keep their records only while something is still running them.
    pub fn adopt(&self, manifests: &ManifestStore) {
        self.replace_build(manifests, None);
    }

    pub fn activate(
        &self,
        manifests: &ManifestStore,
        settings: HashMap<UnitName, HashMap<String, Value>>,
    ) {
        self.replace_build(manifests, Some(settings));
    }

    fn replace_build(
        &self,
        manifests: &ManifestStore,
        settings: Option<HashMap<UnitName, HashMap<String, Value>>>,
    ) {
        {
            let mut units = self.lock();
            for (name, manifest) in manifests.iter() {
                units
                    .entry(name.clone())
                    .or_insert_with(|| UnitRecord::new(name.clone()))
                    .manifest = Some(manifest.clone());
            }

            for record in units.values_mut() {
                if manifests.get(&record.name).is_none() {
                    record.manifest = None;
                }
            }
            if let Some(settings) = settings {
                for (name, config) in settings {
                    if let Some(record) = units.get_mut(&name) {
                        record.config = config;
                    }
                }
            }
            units.retain(|_, record| {
                record.manifest.is_some() || record.is_held() || record.is_connected()
            });
        }
        self.publish();
    }

    /// The manifest the daemon vouches for, if this build has one.
    pub fn manifest(&self, name: &UnitName) -> Option<UnitManifest> {
        self.lock().get(name)?.manifest.clone()
    }

    /// Every unit this build produced.
    pub fn built(&self) -> Vec<UnitName> {
        self.lock()
            .values()
            .filter(|record| record.manifest.is_some())
            .map(|record| record.name.clone())
            .collect()
    }

    // ---- what the document configured ----

    /// The settings to hand a unit that is connecting.
    pub fn config(&self, name: &UnitName) -> HashMap<String, Value> {
        self.lock()
            .get(name)
            .map(|record| record.config.clone())
            .unwrap_or_default()
    }

    // ---- identity ----

    /// Mint the token for a spawn that is about to happen. Issuing it before
    /// the spawn is what makes a fast unit's first connection identifiable.
    pub fn issue(&self, name: &UnitName) -> UnitToken {
        self.mint(name, false)
    }

    /// Mint a token for a process the daemon will not spawn, and hold the
    /// unit's place while that process is it.
    ///
    /// The token is the same kind of token: an adopted unit is admitted, and
    /// granted, exactly as a spawned one — the manifest is still the ceiling.
    /// What differs is only who started the process.
    pub fn adopt_unit(&self, name: &UnitName) -> UnitToken {
        let token = self.mint(name, true);
        self.publish();
        token
    }

    /// Give an adopted unit back to the supervisor, and say so, so that a
    /// convergence starts the built binary again.
    pub fn release_adoption(&self, name: &UnitName, token: &UnitToken) {
        {
            let mut units = self.lock();
            let Some(record) = units.get_mut(name) else {
                return;
            };
            if !record.adopted || record.token.as_ref() != Some(token) {
                return;
            }
            if let Some(session) = record.session.take() {
                session.stop.trigger();
                self.inner.hub.forget_unit(name);
            }
            record.instances.clear();
            record.adopted = false;
            record.token = None;
            record.pid = None;
        }
        self.publish();

        // Best effort, for the same reason a connection is: a full channel
        // means the convergence this would ask for is already pending.
        let _ = self.inner.connected.try_send(name.clone());
    }

    fn mint(&self, name: &UnitName, adopted: bool) -> UnitToken {
        let token = UnitToken::mint();
        let mut units = self.lock();
        let record = units
            .entry(name.clone())
            .or_insert_with(|| UnitRecord::new(name.clone()));
        if let Some(session) = record.session.take() {
            session.stop.trigger();
            self.inner.hub.forget_unit(name);
        }
        record.instances.clear();
        record.token = Some(token.clone());
        record.pid = None;
        record.adopted = adopted;
        token
    }

    pub fn is_adopted(&self, name: &UnitName) -> bool {
        self.lock().get(name).is_some_and(|record| record.adopted)
    }

    /// Retire the current token: its process is gone and it must never be
    /// honoured again, even if the kernel hands that pid to someone else.
    pub fn revoke(&self, name: &UnitName) {
        if let Some(record) = self.lock().get_mut(name) {
            if let Some(session) = record.session.take() {
                session.stop.trigger();
                self.inner.hub.forget_unit(name);
            }
            record.instances.clear();
            record.token = None;
            record.pid = None;
        }
    }

    /// Bind a token to the process that now holds it.
    pub fn bind(&self, name: &UnitName, pid: i32) {
        if let Some(record) = self.lock().get_mut(name) {
            record.pid = Some(pid);
        }
    }

    /// The unit a peer may claim, if its pid and token agree with a live
    /// registration.
    pub fn identify(&self, pid: i32, token: &str) -> Option<UnitName> {
        if token.is_empty() {
            return None;
        }
        let mut units = self.lock();
        units
            .values_mut()
            .find_map(|record| record.claims(pid, token).then(|| record.name.clone()))
    }

    // ---- supervision ----

    pub fn supervise(&self, name: &UnitName, control: UnitControl) {
        let mut units = self.lock();
        units
            .entry(name.clone())
            .or_insert_with(|| UnitRecord::new(name.clone()))
            .control = Some(control);
    }

    pub fn control(&self, name: &UnitName) -> Option<UnitControl> {
        self.lock().get(name)?.control.clone()
    }

    /// The supervisor is done with this unit.
    pub fn release(&self, name: &UnitName) {
        if let Some(record) = self.lock().get_mut(name) {
            record.control = None;
        }
        self.publish();
    }

    /// The units something is running: supervised, or adopted by whoever is
    /// working on them. What the reconciler compares the document against.
    pub fn held(&self) -> Vec<UnitName> {
        self.lock()
            .values()
            .filter(|record| record.is_held())
            .map(|record| record.name.clone())
            .collect()
    }

    pub fn is_supervised(&self, name: &UnitName) -> bool {
        self.lock().get(name).is_some_and(UnitRecord::is_supervised)
    }

    // ---- sessions ----

    /// Register a connected unit until the returned guard is dropped.
    pub fn connected(&self, name: &UnitName, requests: mpsc::Sender<Request>) -> SessionGuard {
        let link = session::SessionLink {
            bytes: self.inner.request_bytes.clone(),
            requests,
            stop: Shutdown::new(),
        };
        {
            let mut units = self.lock();
            let record = units
                .entry(name.clone())
                .or_insert_with(|| UnitRecord::new(name.clone()));
            if let Some(previous) = record.session.replace(link.clone()) {
                previous.stop.trigger();
                self.inner.hub.forget_unit(name);
            }
            // Completing the handshake is what turns a spawned process into a
            // unit the daemon vouches for.
            record.instances.clear();
            record.lifecycle.connected();
        }
        self.publish();

        // Best effort: a full channel means a convergence is already pending,
        // which is what this would have asked for.
        let _ = self.inner.connected.try_send(name.clone());
        SessionGuard::new(self.clone(), name.clone(), link)
    }

    pub(crate) fn disconnected(&self, name: &UnitName, link: &session::SessionLink) {
        {
            let mut units = self.lock();
            let Some(record) = units.get_mut(name) else {
                return;
            };
            if !record
                .session
                .as_ref()
                .is_some_and(|current| current.requests.same_channel(&link.requests))
            {
                return;
            }
            record.session = None;
            record.instances.clear();
            link.stop.trigger();
            self.inner.hub.forget_unit(name);
        }
        self.publish();
    }

    pub fn is_connected(&self, name: &UnitName) -> bool {
        self.lock().get(name).is_some_and(UnitRecord::is_connected)
    }

    /// Invoke an op on a unit and wait for its answer.
    pub async fn request(
        &self,
        unit: &UnitName,
        op: invoke::Op,
    ) -> Result<result::Outcome, RequestError> {
        let session = self
            .lock()
            .get(unit)
            .and_then(|record| record.session.clone())
            .ok_or_else(|| RequestError::Absent(unit.clone()))?;

        Self::request_on(&session, unit, op).await
    }

    async fn request_on(
        session: &session::SessionLink,
        unit: &UnitName,
        op: invoke::Op,
    ) -> Result<result::Outcome, RequestError> {
        let size = op.encoded_len();
        if size > omega_proto::MAX_FRAME_LEN - 32 {
            return Err(RequestError::TooLarge(unit.clone()));
        }
        let bytes = session
            .bytes
            .clone()
            .try_acquire_many_owned(size as u32)
            .map_err(|_| RequestError::Full(unit.clone()))?;
        let slot = session
            .requests
            .try_reserve()
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => RequestError::Full(unit.clone()),
                mpsc::error::TrySendError::Closed(_) => RequestError::Absent(unit.clone()),
            })?;
        let (answer, answered) = oneshot::channel();
        let deadline = tokio::time::Instant::now() + session::REQUEST_TIMEOUT;
        slot.send(Request {
            op,
            answer,
            _bytes: bytes,
        });

        match tokio::time::timeout_at(deadline, answered).await {
            Ok(Ok(Ok(outcome))) => Ok(outcome),
            Ok(Ok(Err(refusal))) => Err(RequestError::Refused {
                unit: unit.clone(),
                source: refusal,
            }),
            // The session ended while the request was outstanding.
            Ok(Err(_)) => Err(RequestError::Absent(unit.clone())),
            Err(_) => Err(RequestError::Timeout(unit.clone())),
        }
    }

    pub fn instances(&self) -> BTreeMap<crate::hub::SurfaceRef, HashMap<String, Value>> {
        self.lock()
            .values()
            .flat_map(|record| record.instances.clone())
            .collect()
    }

    pub async fn configure_instance(
        &self,
        address: &crate::hub::SurfaceRef,
        config: HashMap<String, Value>,
    ) -> Result<(), RequestError> {
        let session = self
            .lock()
            .get(&address.unit)
            .and_then(|record| record.session.clone())
            .ok_or_else(|| RequestError::Absent(address.unit.clone()))?;
        let op = invoke::Op::RenderWidget(omega_proto::omega::RenderWidget {
            surface_id: address.surface.to_string(),
            module_id: address
                .module
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            config: config.clone(),
        });
        let outcome = Self::request_on(&session, &address.unit, op).await?;
        let result::Outcome::View(view) = outcome else {
            return Err(RequestError::Refused {
                unit: address.unit.clone(),
                source: Refusal::invalid("RenderWidget requires a view result"),
            });
        };
        let mut records = self.lock();
        let record = records
            .get_mut(&address.unit)
            .ok_or_else(|| RequestError::Absent(address.unit.clone()))?;
        if !record
            .session
            .as_ref()
            .is_some_and(|current| current.requests.same_channel(&session.requests))
        {
            return Err(RequestError::Absent(address.unit.clone()));
        }
        self.inner
            .hub
            .publish_view(crate::hub::ViewUpdate {
                surface: address.clone(),
                view,
            })
            .map_err(|error| RequestError::Refused {
                unit: address.unit.clone(),
                source: crate::Refusable::refusal(&error),
            })?;
        record.instances.insert(address.clone(), config);
        Ok(())
    }

    pub async fn remove_instance(
        &self,
        address: &crate::hub::SurfaceRef,
    ) -> Result<(), RequestError> {
        let session = self
            .lock()
            .get(&address.unit)
            .and_then(|record| record.session.clone());
        if let Some(session) = session {
            let outcome = Self::request_on(
                &session,
                &address.unit,
                invoke::Op::RemoveWidget(omega_proto::omega::RemoveWidget {
                    surface_id: address.surface.to_string(),
                    module_id: address
                        .module
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                }),
            )
            .await?;
            if !matches!(outcome, result::Outcome::Ok(_)) {
                return Err(RequestError::Refused {
                    unit: address.unit.clone(),
                    source: Refusal::invalid("RemoveWidget requires an ok result"),
                });
            }
            let mut records = self.lock();
            if let Some(record) = records.get_mut(&address.unit)
                && record
                    .session
                    .as_ref()
                    .is_some_and(|current| current.requests.same_channel(&session.requests))
            {
                record.instances.remove(address);
                self.inner.hub.drop_surface(address);
            }
        }
        Ok(())
    }

    // ---- lifecycle ----

    /// Record something that happened to a unit, and publish it if the state
    /// moved.
    pub fn transition(&self, name: &UnitName, transition: Transition) {
        let moved = {
            let mut units = self.lock();
            units
                .entry(name.clone())
                .or_insert_with(|| UnitRecord::new(name.clone()))
                .apply(transition)
        };

        if moved {
            self.publish();
        }
    }

    pub fn lifecycle(&self, name: &UnitName) -> Option<Lifecycle> {
        Some(self.lock().get(name)?.lifecycle.clone())
    }

    pub fn statuses(&self) -> Vec<UnitStatus> {
        self.lock().values().map(UnitRecord::status).collect()
    }

    /// Whether every unit has finished stopping.
    pub fn all_stopped(&self) -> bool {
        self.lock()
            .values()
            .all(|record| record.lifecycle.is_stopped() || !record.is_supervised())
    }

    /// Publish the table as the `units` topic. "This daemon runs no units" is
    /// a fact about the machine like any other, so the topic exists from the
    /// start rather than appearing with the first unit.
    pub fn publish(&self) {
        let records = self.lock();
        let units = records.values().map(UnitRecord::status).collect();
        self.inner
            .hub
            .publish_state(StatePatch {
                topics: vec![StateTopic {
                    topic: SystemTopic::Units.as_str().to_string(),
                    revision: 0, // the Hub assigns the real revision
                    value: Some(state_topic::Value::Units(UnitsState { units })),
                }],
            })
            .unwrap_or_else(|error| tracing::error!(%error, "unit status publication refused"));
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<UnitName, UnitRecord>> {
        // A panic while holding this lock would poison it; the table holds no
        // invariant a panic could break, so recover rather than cascade one
        // unit's failure into the daemon's.
        self.inner.units.lock().unwrap_or_else(|e| e.into_inner())
    }
}
