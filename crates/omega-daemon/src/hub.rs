//! The daemon's shared state and message bus.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use omega_core::{ModuleId, SurfaceId, UnitName};
use omega_wire::omega::{Event, EventKind, StatePatch, StateSnapshot, StateTopic, ViewTree};

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

/// One line on the observation socket: a rendered surface, or a state topic.
///
/// The shell reads the views; `omega status` reads the `units` topic. Both
/// are the same read-only stream, because both are asking the same question —
/// what does the daemon currently hold?
#[derive(Clone, Debug, serde::Serialize)]
#[serde(untagged)]
pub enum Observed {
    View(ViewUpdate),
    State(StateTopic),
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
    state: broadcast::Sender<StatePatch>,
    views: Mutex<ViewRegistry>,
    view_tx: broadcast::Sender<ViewUpdate>,
    events: broadcast::Sender<Event>,
    /// State changes the ontology names as events.
    transitions: Mutex<Transitions>,
    stamp: EventStamp,
}

/// The latest view per surface, plus the per-surface revision counter the
/// hub owns. Views are the same pattern as state: producers publish values,
/// the daemon assigns monotonic revisions.
#[derive(Debug, Default)]
struct ViewRegistry {
    latest: BTreeMap<SurfaceRef, ViewUpdate>,
    revisions: HashMap<SurfaceRef, u64>,
}

impl Hub {
    pub fn new() -> Self {
        let (state, _) = broadcast::channel(64);
        let (view_tx, _) = broadcast::channel(64);
        let (events, _) = broadcast::channel(64);
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
    pub fn publish_state(&self, patch: StatePatch) {
        let changed = self.store().apply(patch);
        if changed.topics.is_empty() {
            return;
        }

        // "The AC was unplugged" and `battery.charging == false` are the same
        // fact; deriving one from the other is what keeps them agreeing.
        let derived = self
            .inner
            .transitions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .of(&changed);

        let _ = self.inner.state.send(changed);

        for kind in derived {
            self.publish_event(self.inner.stamp.stamp(kind, PowerDetail::of(kind)));
        }
    }

    /// Broadcast an event to whoever subscribes to its kind. Events are not
    /// stored: a unit that was not listening missed it, which is what makes
    /// an event an event and not state.
    pub fn publish_event(&self, event: Event) {
        tracing::debug!(id = event.id, kind = ?EventKind::try_from(event.kind), "event");
        let _ = self.inner.events.send(event);
    }

    /// A unit's own event, stamped with the identity the daemon
    /// authenticated.
    pub fn publish_custom_event(
        &self,
        unit: &str,
        name: &str,
        payload: Option<omega_wire::omega::Value>,
    ) {
        self.publish_event(self.inner.stamp.custom(unit, name, payload));
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<Event> {
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
    pub fn publish_view(&self, mut update: ViewUpdate) {
        let mut registry = self.views();

        // Dedupe on content only — the stored tree's revision is the hub's
        // stamp, not part of the content.
        if registry
            .latest
            .get(&update.surface)
            .is_some_and(|prev| prev.view.root == update.view.root)
        {
            return;
        }

        let revision = registry
            .revisions
            .entry(update.surface.clone())
            .or_default();
        *revision += 1;
        update.view.revision = *revision;

        registry
            .latest
            .insert(update.surface.clone(), update.clone());
        drop(registry);

        let _ = self.inner.view_tx.send(update);
    }

    /// Forget the anonymous instance of a surface. Once the document
    /// instantiates a surface, the unit's own single view is a leftover from
    /// before it was told about its instances.
    pub fn drop_anonymous(&self, unit: &UnitName, surface: &SurfaceId) {
        let anonymous = SurfaceRef::new(unit.clone(), surface.clone());
        let mut registry = self.views();
        registry.latest.remove(&anonymous);
        registry.revisions.remove(&anonymous);
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
        let dropped: Vec<SurfaceRef> = {
            let mut registry = self.views();
            let dropped: Vec<SurfaceRef> = registry
                .latest
                .keys()
                .filter(|surface| &surface.unit == unit)
                .cloned()
                .collect();

            for surface in &dropped {
                registry.latest.remove(surface);
                registry.revisions.remove(surface);
            }
            dropped
        };

        for surface in dropped {
            let _ = self.inner.view_tx.send(ViewUpdate {
                surface,
                view: ViewTree::default(),
            });
        }
    }

    /// Forget a module's view: no bar declares that instance any more, so
    /// the shell should stop being told about it.
    pub fn drop_view(&self, module: &ModuleId) {
        let wanted = Some(module.clone());
        let mut registry = self.views();
        registry
            .latest
            .retain(|surface, _| surface.module != wanted);
        registry
            .revisions
            .retain(|surface, _| surface.module != wanted);
    }

    /// Snapshot + subscription, taken under one lock so a publish can never
    /// fall in the gap between them (it is either in the snapshot or arrives
    /// on the receiver).
    pub fn subscribe_state(&self) -> (StateSnapshot, broadcast::Receiver<StatePatch>) {
        let store = self.store();
        (store.snapshot(), self.inner.state.subscribe())
    }

    /// Snapshot + view subscription, atomic like [`Self::subscribe_state`].
    pub fn subscribe_views(&self) -> (Vec<ViewUpdate>, broadcast::Receiver<ViewUpdate>) {
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
    pub fn view_snapshot(&self) -> Vec<ViewUpdate> {
        self.inner
            .views
            .lock()
            .unwrap()
            .latest
            .values()
            .cloned()
            .collect()
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}
