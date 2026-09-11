//! The daemon's shared state and message bus.

pub mod history;
use history::{History, Receiver};
use prost::Message;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use omega_proto::omega::{
    Event, EventKind, ScheduleFired, StatePatch, StateSnapshot, ViewTree, event,
};
use omega_proto::{ModuleId, SurfaceId, UnitName};

use crate::events::{EventStamp, PowerDetail, Transitions};
use crate::state::StateStore;

/// Which instance, of which surface, of which unit.
///
/// Surface ids are chosen by unit authors, so they are only unique within a
/// unit: two units may both call a surface "battery". The daemon qualifies
/// every published view with the unit it authenticated, which is what keeps
/// one unit from writing over another's slot in the bar.
///
/// The module is the third dimension: one surface can be instantiated more
/// than once — two clocks with different formats — and each instance is its
/// own view. No module is the surface's single instance, which is what a unit
/// that never heard of modules publishes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct SurfaceRef {
    pub unit: UnitName,
    pub surface: SurfaceId,
    /// Absent rather than empty: "the only instance" is a different thing
    /// from "an instance called nothing".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<ModuleId>,
}

impl SurfaceRef {
    /// A surface's single instance.
    pub fn new(unit: UnitName, surface: SurfaceId) -> Self {
        Self {
            unit,
            surface,
            module: None,
        }
    }

    /// One instance of a surface, as the document declared it.
    pub fn module(unit: UnitName, surface: SurfaceId, module: ModuleId) -> Self {
        Self {
            unit,
            surface,
            module: Some(module),
        }
    }

    pub fn is_instance(&self) -> bool {
        self.module.is_some()
    }
}

impl std::fmt::Display for SurfaceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.module {
            None => write!(f, "{}.{}", self.unit, self.surface),
            Some(module) => write!(f, "{}.{}#{module}", self.unit, self.surface),
        }
    }
}

/// A published view: a surface's latest declarative tree.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ViewUpdate {
    #[serde(flatten)]
    pub surface: SurfaceRef,
    pub view: ViewTree,
}

/// Proof the daemon is still on the other end.
///
/// An observer cannot tell a quiet daemon from a dead one: a peer that goes
/// away leaves the socket reading connected, so the shell watches for silence
/// instead and reconnects through it. That made incidental traffic load-
/// bearing — the shell stayed up because some topic happened to change every
/// couple of seconds — and an observer that narrows its subscription is an
/// observer that has just turned its own keepalive off.
///
/// So the socket says it out loud on a cadence of its own, and liveness stops
/// depending on how busy the machine is.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Heartbeat {
    /// Always true. A field, because every line on this socket is a JSON
    /// object and an observer tells them apart by which keys they carry.
    pub heartbeat: bool,
}

/// A handle to the daemon's authoritative state and message channels.
///
/// One clone is handed to each session, source, and the shell server; the
/// daemon core holds the original. All methods are sync — the mutex never
/// crosses an await.
#[derive(Debug, Clone)]
pub struct Hub {
    inner: Arc<HubInner>,
}

#[derive(Debug)]
struct HubInner {
    store: Mutex<StateStore>,
    state: History<StatePatch>,
    views: Mutex<ViewRegistry>,
    view_tx: History<Arc<ViewUpdate>>,
    events: History<Event>,
    /// State changes the ontology names as events.
    transitions: Mutex<Transitions>,
    stamp: EventStamp,
}

/// Current views and a global sequence that survives removal of any address.
#[derive(Debug, Default)]
struct ViewRegistry {
    latest: BTreeMap<SurfaceRef, Arc<ViewUpdate>>,
    revision: u64,
    bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error(transparent)]
    State(#[from] crate::state::StateError),
    #[error("publication exceeds payload limit")]
    TooLarge,
    #[error("retained view capacity exhausted")]
    Full,
    #[error("view revision exhausted")]
    RevisionExhausted,
}

impl ViewUpdate {
    fn size(&self) -> usize {
        self.view
            .root
            .as_ref()
            .map_or(0, |root| root.encoded_len() + 8)
            + self.surface.unit.as_str().len()
            + self.surface.surface.as_str().len()
            + self
                .surface
                .module
                .as_ref()
                .map_or(0, |module| module.as_str().len())
            + 64
    }
}

impl Hub {
    const VIEW_BYTES: usize = 8 * 1024 * 1024;
    const VIEW_COUNT: usize = 4096;
    const PUBLICATION_BYTES: usize = omega_proto::MAX_FRAME_LEN - 256;
    pub fn new() -> Self {
        let state = History::new();
        let view_tx = History::new();
        let events = History::new();
        Self {
            inner: Arc::new(HubInner {
                store: Mutex::new(StateStore::new()),
                state,
                views: Mutex::new(ViewRegistry::default()),
                view_tx,
                events,
                transitions: Mutex::new(Transitions::new()),
                stamp: EventStamp::new(),
            }),
        }
    }

    /// The state store. A panic while holding either lock would poison it;
    /// neither guards an invariant a panic could break, so one unit's failure
    /// does not become the daemon's.
    fn store(&self) -> std::sync::MutexGuard<'_, StateStore> {
        self.inner.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn views(&self) -> std::sync::MutexGuard<'_, ViewRegistry> {
        self.inner.views.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Store a patch (assigning revisions) and broadcast what changed.
    /// Sessions filter it against their own subscriptions, so the hub
    /// broadcasts patches rather than frames.
    pub fn publish_state(&self, patch: StatePatch) -> Result<(), PublishError> {
        let mut store = self.store();
        let changed = store.apply(patch)?;
        if changed.topics.is_empty() {
            return Ok(());
        }

        // Commit and enqueue while holding the store lock, preserving revision order.
        let derived = self
            .inner
            .transitions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .of(&changed);

        let size = changed.encoded_len();
        self.inner.state.send(changed, size);

        for kind in derived {
            self.publish_event(self.inner.stamp.stamp(kind, PowerDetail::of(kind)))
                .expect("derived events have bounded payloads");
        }
        Ok(())
    }

    /// Broadcast an event to whoever subscribes to its kind. Events are not
    /// stored: a unit that was not listening missed it, which is what makes
    /// an event an event and not state.
    pub fn publish_event(&self, event: Event) -> Result<(), PublishError> {
        let size = event.encoded_len();
        if size > Self::PUBLICATION_BYTES {
            return Err(PublishError::TooLarge);
        }
        tracing::debug!(id = event.id, kind = ?EventKind::try_from(event.kind), "event");
        self.inner.events.send(event, size);
        Ok(())
    }

    /// A schedule fired. The daemon's own clock speaking, so the id is the
    /// document's and there is no peer to attribute it to.
    pub fn publish_schedule_fired(&self, schedule_id: &str) -> Result<(), PublishError> {
        self.publish_event(self.inner.stamp.stamp(
            EventKind::EventScheduleFired,
            Some(event::Detail::Schedule(ScheduleFired {
                schedule_id: schedule_id.to_string(),
            })),
        ))
    }

    /// A unit's own event, stamped with the identity the daemon
    /// authenticated.
    pub fn publish_custom_event(
        &self,
        unit: &str,
        name: &str,
        payload: Option<omega_proto::omega::Value>,
    ) -> Result<(), PublishError> {
        self.publish_event(self.inner.stamp.custom(unit, name, payload))
    }

    pub fn subscribe_events(&self) -> Receiver<Event> {
        self.inner.events.subscribe()
    }

    /// The current value of the named topics (empty means all).
    pub fn read_state(&self, topics: &[String]) -> StatePatch {
        self.store().read(topics)
    }

    /// Store the latest view for a surface and broadcast it to shell
    /// subscribers.
    ///
    /// The hub owns the `revision`: it stamps each *changed* tree with the
    /// next monotonic value for that surface. Identical re-publishes are
    /// deduplicated (no new revision, no broadcast).
    pub fn publish_view(&self, mut update: ViewUpdate) -> Result<(), PublishError> {
        let size = update.size();
        if size > Self::PUBLICATION_BYTES {
            return Err(PublishError::TooLarge);
        }
        let mut registry = self.views();

        if registry
            .latest
            .get(&update.surface)
            .is_some_and(|prev| prev.view.root == update.view.root)
        {
            return Ok(());
        }
        let previous = registry.latest.get(&update.surface);
        let bytes = registry.bytes - previous.map_or(0, |view| view.size()) + size;
        let count = registry.latest.len() + usize::from(previous.is_none());
        if bytes > Self::VIEW_BYTES || count > Self::VIEW_COUNT {
            return Err(PublishError::Full);
        }
        // A global sequence needs no tombstone map and never reuses a removed
        // surface's revision, even when that address is published again.
        registry.revision = registry
            .revision
            .checked_add(1)
            .ok_or(PublishError::RevisionExhausted)?;
        update.view.revision = registry.revision;
        let update = Arc::new(update);
        registry
            .latest
            .insert(update.surface.clone(), update.clone());
        registry.bytes = bytes;
        self.inner.view_tx.send(update, size);
        Ok(())
    }

    /// Forget everything a unit was showing, because it has gone.
    ///
    /// A view is what a unit is *currently* saying. Keeping the last one
    /// after its process ends leaves a widget on the bar reporting a number
    /// nobody is taking — and, worse, tells the reconciler that the unit
    /// still knows about the instances the document gave it. It does not:
    /// that knowledge lived in the process, and the process is gone. Left
    /// alone, the unit comes back, is never told about its instances again,
    /// and the bar shows its last frame forever.
    ///
    /// Observers are told, with an empty tree for each surface: a shell that
    /// is not told cannot know to stop drawing.
    pub fn forget_unit(&self, unit: &UnitName) {
        let mut registry = self.views();
        let dropped: Vec<_> = registry
            .latest
            .keys()
            .filter(|surface| &surface.unit == unit)
            .cloned()
            .collect();
        for surface in dropped {
            self.remove_view(&mut registry, &surface);
        }
    }

    pub fn drop_view(&self, module: &ModuleId) {
        let mut registry = self.views();
        let dropped: Vec<_> = registry
            .latest
            .keys()
            .filter(|surface| surface.module.as_ref() == Some(module))
            .cloned()
            .collect();
        for surface in dropped {
            self.remove_view(&mut registry, &surface);
        }
    }

    pub fn drop_surface(&self, surface: &SurfaceRef) {
        self.remove_view(&mut self.views(), surface);
    }

    fn remove_view(&self, registry: &mut ViewRegistry, surface: &SurfaceRef) {
        let Some(previous) = registry.latest.remove(surface) else {
            return;
        };
        registry.bytes -= previous.size();
        registry.revision = registry
            .revision
            .checked_add(1)
            .expect("view revision exhausted");
        let update = ViewUpdate {
            surface: surface.clone(),
            view: ViewTree {
                root: None,
                revision: registry.revision,
            },
        };
        let size = update.size();
        self.inner.view_tx.send(Arc::new(update), size);
    }

    /// Snapshot + subscription, taken under one lock so a publish can never
    /// fall in the gap between them (it is either in the snapshot or arrives
    /// on the receiver).
    pub fn subscribe_state(&self) -> (StateSnapshot, Receiver<StatePatch>) {
        let store = self.store();
        (store.snapshot(), self.inner.state.subscribe())
    }

    /// Snapshot + view subscription, atomic like [`Self::subscribe_state`].
    /// Published values are immutable; snapshots and deliveries share ownership.
    pub fn subscribe_views(&self) -> (Vec<Arc<ViewUpdate>>, Receiver<Arc<ViewUpdate>>) {
        let registry = self.views();
        (
            registry.latest.values().cloned().collect(),
            self.inner.view_tx.subscribe(),
        )
    }

    pub fn snapshot(&self) -> StateSnapshot {
        self.store().snapshot()
    }

    /// The latest view for every surface — deterministic (key-sorted) order.
    pub fn view_snapshot(&self) -> Vec<Arc<ViewUpdate>> {
        self.views().latest.values().cloned().collect()
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture;
    impl Fixture {
        fn view(id: &str, size: usize) -> ViewUpdate {
            ViewUpdate {
                surface: SurfaceRef::new(
                    UnitName::parse("example").unwrap(),
                    SurfaceId::parse(id).unwrap(),
                ),
                view: ViewTree {
                    root: Some(omega_proto::omega::ViewNode {
                        key: "x".repeat(size),
                        ..Default::default()
                    }),
                    revision: 0,
                },
            }
        }
    }

    #[tokio::test]
    async fn view_saturation_is_transactional_and_removal_releases_capacity() {
        let hub = Hub::new();
        hub.publish_view(Fixture::view("first", 3_000_000)).unwrap();
        hub.publish_view(Fixture::view("second", 3_000_000))
            .unwrap();
        let (_, mut updates) = hub.subscribe_views();
        let revision = hub.views().revision;
        assert!(matches!(
            hub.publish_view(Fixture::view("third", 3_000_000)),
            Err(PublishError::Full)
        ));
        assert_eq!(hub.views().revision, revision);
        assert!(updates.try_recv().is_err());
        let first = Fixture::view("first", 0).surface;
        hub.drop_surface(&first);
        let removed = updates.recv().await.unwrap();
        hub.publish_view(Fixture::view("first", 3_000_000)).unwrap();
        let recreated = updates.recv().await.unwrap();
        assert!(recreated.view.revision > removed.view.revision);
        assert!(hub.views().bytes <= Hub::VIEW_BYTES);
    }

    #[tokio::test]
    async fn view_history_lag_recovers_an_atomic_snapshot_without_deleted_views() {
        let hub = Hub::new();
        let (_, mut updates) = hub.subscribe_views();
        hub.publish_view(Fixture::view("old", 3_000_000)).unwrap();
        hub.drop_surface(&Fixture::view("old", 0).surface);
        for size in 3_000_000..3_000_004 {
            hub.publish_view(Fixture::view("current", size)).unwrap();
        }
        assert!(matches!(
            updates.recv().await,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
        ));
        let (snapshot, mut updates) = hub.subscribe_views();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].surface.surface.as_str(), "current");
        hub.publish_view(Fixture::view("current", 1)).unwrap();
        assert!(updates.recv().await.unwrap().view.revision > snapshot[0].view.revision);
    }

    #[test]
    fn transient_view_addresses_leave_no_revision_registry() {
        let hub = Hub::new();
        for id in 0..Hub::VIEW_COUNT + 1 {
            let view = Fixture::view(&format!("view{id}"), 0);
            let surface = view.surface.clone();
            hub.publish_view(view).unwrap();
            hub.drop_surface(&surface);
        }
        assert!(hub.views().latest.is_empty());
        assert_eq!(hub.views().bytes, 0);
        assert_eq!(hub.views().revision, 2 * (Hub::VIEW_COUNT as u64 + 1));
    }

    #[test]
    fn view_count_and_individual_event_size_are_bounded() {
        let hub = Hub::new();
        for id in 0..Hub::VIEW_COUNT {
            hub.publish_view(Fixture::view(&format!("view{id}"), 0))
                .unwrap();
        }
        assert!(matches!(
            hub.publish_view(Fixture::view("extra", 0)),
            Err(PublishError::Full)
        ));
        assert!(matches!(
            hub.publish_event(Event {
                detail: Some(event::Detail::Schedule(ScheduleFired {
                    schedule_id: "x".repeat(Hub::PUBLICATION_BYTES)
                })),
                ..Default::default()
            }),
            Err(PublishError::TooLarge)
        ));
    }

    #[tokio::test]
    async fn views_share_storage_and_release_it_after_eviction_and_final_reader() {
        let hub = Hub::new();
        let (_, mut first) = hub.subscribe_views();
        let (_, mut second) = hub.subscribe_views();
        hub.publish_view(Fixture::view("shared", 600_000)).unwrap();
        let first = first.recv().await.unwrap();
        let second = second.recv().await.unwrap();
        let snapshot = hub.view_snapshot();
        assert!(Arc::ptr_eq(&first, &second));
        assert!(Arc::ptr_eq(&first, &snapshot[0]));
        let weak = Arc::downgrade(&first);
        let original_revision = first.view.revision;
        for size in 1..=70 {
            hub.publish_view(Fixture::view("shared", size)).unwrap();
        }
        assert_eq!(first.view.revision, original_revision);
        assert_eq!(first.view.root.as_ref().unwrap().key.len(), 600_000);
        assert!(hub.view_snapshot()[0].view.revision > original_revision);
        drop(snapshot);
        drop(second);
        assert!(weak.upgrade().is_some());
        drop(first);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn a_poisoned_lock_does_not_take_the_daemon_with_it() {
        // The policy the accessors above exist to hold, asserted rather than
        // stated: neither lock guards an invariant a panic could break, so
        // one panicked update must not turn every later read into a second
        // panic. `view_snapshot` took its own lock and unwrapped it, which is
        // the observer resync path — one bad view would have ended every
        // observer connection instead of one.
        let hub = Hub::new();

        let hushed = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _views = hub.inner.views.lock().unwrap();
            let _store = hub.inner.store.lock().unwrap();
            panic!("an update panicked while holding both");
        }));
        std::panic::set_hook(hushed);
        assert!(panicked.is_err(), "the test did not poison anything");

        // Every reader still answers.
        let _ = hub.view_snapshot();
        let _ = hub.snapshot();
        let _ = hub.subscribe_views();
        let _ = hub.subscribe_state();
    }
}
