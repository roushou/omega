use omega::{
    Surface, View,
    storage::{Query, Revision, Snapshot, Storage, StoragePolicy, Store, Subscribed, Subscription},
    surface::{Events, Task},
    testing::{State, Stored, SurfaceHarness},
    ui::Text,
};
use std::convert::Infallible;

struct Tasks;
impl Storage for Tasks {
    type Key = String;
    type Value = String;
    const ID: &'static str = "example.tasks";
    const POLICY: StoragePolicy = StoragePolicy::Memory;
}
struct FirstTask;
impl Subscription for FirstTask {
    type Storage = Tasks;
    fn query(&self) -> Query<Tasks> {
        Query::new().limit(1)
    }
}
#[derive(omega::Effects)]
struct Writes {
    tasks: Store<Tasks>,
}
#[derive(omega::Surface)]
struct List {
    tasks: Subscribed<FirstTask>,
}
impl Surface for List {
    type Model = ();
    type Message = Infallible;
    type Effects = Writes;
    fn mounted(&self, _: &mut (), effects: &Writes) -> Task<Infallible> {
        let _ = &effects.tasks;
        Task::none()
    }
    fn initialize(&mut self, _: &mut ()) -> omega::Result<()> {
        self.tasks.start(FirstTask)
    }
    fn render(&self, _: &(), _: &Events<Infallible>) -> View {
        Text::new(match self.tasks.snapshot() {
            Snapshot::Loading => "loading".into(),
            Snapshot::Failed(error) => error,
            Snapshot::Ready(page) => page
                .entries()
                .iter()
                .map(|entry| entry.value.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        })
        .into()
    }
    fn update(&self, _: &mut (), message: Infallible, _: &Writes) -> Task<Infallible> {
        match message {}
    }
}

#[tokio::test]
async fn query_selection_loading_failure_and_close_use_production_lifecycle() {
    let mut harness = SurfaceHarness::<List>::new(&State::new()).unwrap();
    assert_eq!(harness.draw().text(), "loading");
    let effect = harness.take_effect().unwrap();
    assert!(matches!(
        effect.operation(),
        omega_proto::omega::invoke::Op::StorageSubscribe(_)
    ));
    effect.complete(Ok(None)).unwrap();
    let snapshot = Stored::<Tasks>::new(Revision {
        epoch: "a".repeat(32),
        revision: 2,
    })
    .unwrap()
    .entry("two".into(), "second".into(), 2)
    .unwrap()
    .entry("one".into(), "first".into(), 1)
    .unwrap();
    harness.storage(&snapshot).unwrap();
    assert_eq!(harness.draw().text(), "first");
    harness
        .lifecycle(omega::surface::Lifecycle::Hidden)
        .unwrap();
    assert!(harness.take_effect().is_none());
    harness
        .lifecycle(omega::surface::Lifecycle::Closed)
        .unwrap();
    let cleanup = harness.take_effect().unwrap();
    assert!(matches!(
        cleanup.operation(),
        omega_proto::omega::invoke::Op::StorageUnsubscribe(_)
    ));
    cleanup.complete(Ok(None)).unwrap();
    assert!(harness.storage(&snapshot).is_err());

    let mut refused = SurfaceHarness::<List>::new(&State::new()).unwrap();
    refused
        .take_effect()
        .unwrap()
        .complete(Err(omega::effect::EffectError::Refused(
            omega_proto::Refusal::denied("no access"),
        )))
        .unwrap();
    assert!(refused.draw().text().contains("no access"));
}

#[test]
fn fields_derive_storage_access_including_surface_effects_without_system_grants() {
    let manifest = omega::plugin::Plugin::named("tasks", "1.0.0")
        .surface_as::<List>("list")
        .manifest()
        .unwrap();
    let contracts = omega_proto::storage::StorageContracts::collect(&manifest.storage).unwrap();
    assert_eq!(contracts.len(), 1);
    assert!(contracts.values().next().unwrap().writable);
}

#[derive(omega::Surface)]
struct Unstarted {
    tasks: Subscribed<FirstTask>,
}
impl Surface for Unstarted {
    type Model = ();
    type Message = Infallible;
    type Effects = ();
    fn render(&self, _: &(), _: &Events<Infallible>) -> View {
        let _ = self.tasks.snapshot();
        View::empty()
    }
    fn update(&self, _: &mut (), message: Infallible, _: &()) -> Task<Infallible> {
        match message {}
    }
}
#[test]
fn omitted_initialization_is_an_error_before_render() {
    let error = SurfaceHarness::<Unstarted>::new(&State::new()).unwrap_err();
    assert!(error.to_string().contains("not started"));
}
