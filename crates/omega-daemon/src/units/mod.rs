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

use omega_proto::SystemTopic;
use omega_proto::UnitName;
use omega_proto::omega::{
    StatePatch, StateTopic, UnitStatus, UnitsState, Value, invoke, result, state_topic,
};

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
    hub: Hub,
    /// Announces a unit becoming reachable. A unit is started before it can
    /// be asked for anything, so whoever wants to ask has to be told when it
    /// arrives.
    connected: mpsc::Sender<UnitName>,
}

impl UnitTable {
    /// A table, and the stream of units connecting to it. Only the daemon
    /// holds the receiving end.
    pub fn new(hub: Hub) -> (Self, mpsc::Receiver<UnitName>) {
        let (connected, arrivals) = mpsc::channel(16);
        (
            Self {
                inner: Arc::new(Inner {
                    units: Mutex::new(BTreeMap::new()),
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

    /// Record the settings the document gives this unit, and say whether that
    /// was a change.
    ///
    /// A unit is told its settings once, in its `Welcome`, because its fields
    /// are built out of them. So this is written *before* the unit is spawned
    /// and read when it connects; that a change here means running the unit
    /// again is the reconciler's business, not this table's.
    ///
    /// Only a unit this build produced can be configured. A document naming
    /// one that does not exist is already reported by the provider that knows
    /// what was built, and inventing a record here would leave a unit in the
    /// table that nothing can ever run.
    pub fn configure(&self, name: &UnitName, config: HashMap<String, Value>) -> bool {
        let mut units = self.lock();
        let Some(record) = units.get_mut(name) else {
            return false;
        };

        match record.config == config {
            true => false,
            false => {
                record.config = config;
                true
            }
        }
    }

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
    pub fn release_adoption(&self, name: &UnitName) {
        {
            let mut units = self.lock();
            let Some(record) = units.get_mut(name) else {
                return;
            };
            if !record.adopted {
                return;
            }
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
        record.token = Some(token.clone());
        record.pid = None;
        record.adopted = adopted;
        token
    }

    /// Retire the current token: its process is gone and it must never be
    /// honoured again, even if the kernel hands that pid to someone else.
    pub fn revoke(&self, name: &UnitName) {
        if let Some(record) = self.lock().get_mut(name) {
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
        {
            let mut units = self.lock();
            let record = units
                .entry(name.clone())
                .or_insert_with(|| UnitRecord::new(name.clone()));
            record.session = Some(requests);
            // Completing the handshake is what turns a spawned process into a
            // unit the daemon vouches for.
            record.lifecycle.connected();
        }
        self.publish();

        // Best effort: a full channel means a convergence is already pending,
        // which is what this would have asked for.
        let _ = self.inner.connected.try_send(name.clone());
        SessionGuard::new(self.clone(), name.clone())
    }

    pub(crate) fn disconnected(&self, name: &UnitName) {
        if let Some(record) = self.lock().get_mut(name) {
            record.session = None;
        }

        // What it was showing went with it. A unit's views outliving its
        // process is how a bar ends up frozen: the reconciler reads them as
        // "this unit knows about its instances", and the unit that comes back
        // knows nothing of the sort.
        self.inner.hub.forget_unit(name);
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

        let (answer, answered) = oneshot::channel();
        session
            .send(Request { op, answer })
            .await
            .map_err(|_| RequestError::Absent(unit.clone()))?;

        match tokio::time::timeout(session::REQUEST_TIMEOUT, answered).await {
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
        let units = self.statuses();
        self.inner.hub.publish_state(StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Units.as_str().to_string(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::Units(UnitsState { units })),
            }],
        });
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<UnitName, UnitRecord>> {
        // A panic while holding this lock would poison it; the table holds no
        // invariant a panic could break, so recover rather than cascade one
        // unit's failure into the daemon's.
        self.inner.units.lock().unwrap_or_else(|e| e.into_inner())
    }
}
