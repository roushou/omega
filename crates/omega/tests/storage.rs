use omega::{
    Surface, View,
    storage::{Query, Revision, Snapshot, Storage, StoragePolicy, Store, Subscribed, Subscription},
    surface::{Events, Task},
    testing::{State, Stored, SurfaceHarness},
    ui::Text,
};
use std::convert::Infallible;

#[derive(Debug)]
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

#[derive(Debug)]
enum Outcome {
    Found(Option<omega::storage::Entry<String, String>>),
    Listed(omega::storage::Page<Tasks>),
    Written(Revision),
}

enum Message {
    Read(String),
    Enumerate,
    Insert,
    Replace(Revision),
    Remove(Revision),
    Finished(omega::Result<Outcome>),
}

#[derive(omega::Surface)]
struct Editor {
    tasks: Subscribed<FirstTask>,
}

impl Surface for Editor {
    type Model = Option<omega::Result<Outcome>>;
    type Message = Message;
    type Effects = Writes;

    fn initialize(&mut self, _: &mut Self::Model) -> omega::Result<()> {
        self.tasks.start(FirstTask)
    }

    fn update(&self, model: &mut Self::Model, message: Message, effects: &Writes) -> Task<Message> {
        if let Message::Finished(result) = message {
            *model = Some(result);
            return Task::none();
        }
        let store = effects.tasks.clone();
        Task::perform(
            async move {
                match message {
                    Message::Read(key) => store.get(&key).await.map(Outcome::Found),
                    Message::Enumerate => store
                        .list(Query::new().after(&"a".into()).limit(1))
                        .await
                        .map(Outcome::Listed),
                    Message::Insert => store
                        .insert("one".into(), "Buy coffee".into())
                        .await
                        .map(Outcome::Written),
                    Message::Replace(revision) => store
                        .replace("one".into(), revision, "Buy tea".into())
                        .await
                        .map(Outcome::Written),
                    Message::Remove(revision) => store
                        .remove("one".into(), revision)
                        .await
                        .map(Outcome::Written),
                    Message::Finished(result) => result,
                }
            },
            Message::Finished,
        )
    }

    fn render(&self, _: &Self::Model, _: &Events<Message>) -> View {
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
}

struct Fixture;
impl Fixture {
    fn revision(n: u64) -> Revision {
        Revision {
            epoch: "a".repeat(32),
            revision: n,
        }
    }
    fn editor() -> SurfaceHarness<Editor> {
        let mut panel = SurfaceHarness::<Editor>::new(&State::new()).unwrap();
        panel.take_effect().unwrap().complete(Ok(None)).unwrap();
        panel
    }
    fn written(panel: &SurfaceHarness<Editor>, revision: u64) {
        let Some(Ok(Outcome::Written(actual))) = panel.model() else {
            panic!("expected write outcome")
        };
        assert_eq!(actual, &Self::revision(revision));
    }
    fn closed(panel: &SurfaceHarness<Editor>) {
        assert!(matches!(
            panel.model(),
            Some(Err(omega::Error::Effect(
                omega::effect::EffectError::Closed
            )))
        ));
    }
}

#[tokio::test]
async fn typed_reads_select_exact_keys_absence_and_bounded_pages() {
    let mut panel = Fixture::editor();
    let snapshot = Stored::<Tasks>::new(Fixture::revision(9))
        .unwrap()
        .entry("a".into(), "first".into(), 1)
        .unwrap()
        .entry("b".into(), "second".into(), 2)
        .unwrap()
        .entry("c".into(), "third".into(), 3)
        .unwrap();
    panel.send(Message::Read("b".into())).unwrap();
    let read = panel.expect_storage_read::<Tasks>().await.unwrap();
    assert_eq!(read.query().key().map(String::as_str), Some("b"));
    assert_eq!(read.query().after(), None);
    assert_eq!(read.query().limit(), 1);
    read.reply(&snapshot).unwrap();
    panel.complete().await.unwrap();
    let Some(Ok(Outcome::Found(Some(entry)))) = panel.model() else {
        panic!("expected entry")
    };
    assert_eq!(entry.value, "second");
    assert_eq!(entry.revision, Fixture::revision(2));

    panel.send(Message::Read("absent".into())).unwrap();
    panel
        .expect_storage_read::<Tasks>()
        .await
        .unwrap()
        .reply(&snapshot)
        .unwrap();
    panel.complete().await.unwrap();
    assert!(matches!(panel.model(), Some(Ok(Outcome::Found(None)))));

    panel.send(Message::Enumerate).unwrap();
    let read = panel.expect_storage_read::<Tasks>().await.unwrap();
    assert_eq!(read.query().key(), None);
    assert_eq!(read.query().after().map(String::as_str), Some("a"));
    assert_eq!(read.query().limit(), 1);
    read.reply(&snapshot).unwrap();
    panel.complete().await.unwrap();
    let Some(Ok(Outcome::Listed(page))) = panel.model() else {
        panic!("expected page")
    };
    assert_eq!(page.entries().len(), 1);
    assert_eq!(page.entries()[0].key, "b");
    assert_eq!(page.total, 3);
    assert!(page.truncated);
    assert_eq!(page.revision, Fixture::revision(9));
}

#[tokio::test]
async fn insertion_acknowledgement_and_subscription_delivery_are_independent_in_both_orders() {
    for snapshot_first in [false, true] {
        let mut panel = Fixture::editor();
        panel.send(Message::Insert).unwrap();
        let insert = panel.expect_storage_insert::<Tasks>().await.unwrap();
        assert_eq!(insert.key(), "one");
        assert_eq!(insert.value(), "Buy coffee");
        assert!(panel.model().is_none());
        assert_eq!(panel.draw().text(), "loading");
        let snapshot = Stored::<Tasks>::new(Fixture::revision(1))
            .unwrap()
            .entry("one".into(), "Buy coffee".into(), 1)
            .unwrap();
        if snapshot_first {
            panel.storage(&snapshot).unwrap();
            assert_eq!(panel.draw().text(), "Buy coffee");
            assert!(panel.model().is_none());
        }
        insert.succeed(Fixture::revision(1)).unwrap();
        panel.complete().await.unwrap();
        Fixture::written(&panel, 1);
        if !snapshot_first {
            assert_eq!(panel.draw().text(), "loading");
            panel.storage(&snapshot).unwrap();
            assert_eq!(panel.draw().text(), "Buy coffee");
        }
    }
}

#[tokio::test]
async fn conditional_writes_expose_expected_tokens_and_preserve_noop_revisions() {
    let mut panel = Fixture::editor();
    for revision in [3, 8] {
        panel.send(Message::Replace(Fixture::revision(3))).unwrap();
        let replace = panel.expect_storage_replace::<Tasks>().await.unwrap();
        assert_eq!(replace.key(), "one");
        assert_eq!(replace.value(), "Buy tea");
        assert_eq!(replace.expected(), &Fixture::revision(3));
        replace.succeed(Fixture::revision(revision)).unwrap();
        panel.complete().await.unwrap();
        Fixture::written(&panel, revision);
    }
    panel.send(Message::Remove(Fixture::revision(8))).unwrap();
    let remove = panel.expect_storage_remove::<Tasks>().await.unwrap();
    assert_eq!(remove.key(), "one");
    assert_eq!(remove.expected(), &Fixture::revision(8));
    remove.succeed(Fixture::revision(10)).unwrap();
    panel.complete().await.unwrap();
    Fixture::written(&panel, 10);
}

struct OtherTasks;
impl Storage for OtherTasks {
    type Key = String;
    type Value = String;
    const ID: &'static str = "other.tasks";
    const POLICY: StoragePolicy = StoragePolicy::Memory;
}

#[tokio::test]
async fn mismatches_consume_only_the_next_effect_and_close_its_receipt() {
    let mut panel = Fixture::editor();
    panel.send(Message::Insert).unwrap();
    let error = panel.expect_storage_read::<Tasks>().await.unwrap_err();
    assert!(error.to_string().contains("got insert"));
    panel.complete().await.unwrap();
    Fixture::closed(&panel);

    panel.send(Message::Read("one".into())).unwrap();
    let error = panel.expect_storage_read::<OtherTasks>().await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("expected storage other.tasks, got example.tasks")
    );
    panel.complete().await.unwrap();
    Fixture::closed(&panel);

    panel.send(Message::Insert).unwrap();
    panel.send(Message::Remove(Fixture::revision(1))).unwrap();
    assert!(panel.expect_storage_read::<Tasks>().await.is_err());
    let remove = panel.expect_storage_remove::<Tasks>().await.unwrap();
    assert_eq!(remove.key(), "one");
    remove.succeed(Fixture::revision(2)).unwrap();

    let mut initial = SurfaceHarness::<Editor>::new(&State::new()).unwrap();
    let error = initial.expect_storage_read::<Tasks>().await.unwrap_err();
    assert!(error.to_string().contains("non-storage effect"));
    assert!(initial.take_effect().is_none());
}

#[tokio::test]
async fn refusal_failures_and_dropped_captures_reach_the_plugin() {
    use omega::{effect::EffectError, testing::operation::Refusal};
    let mut panel = Fixture::editor();
    panel.send(Message::Read("one".into())).unwrap();
    panel
        .expect_storage_read::<Tasks>()
        .await
        .unwrap()
        .refuse(Refusal::denied("no access"))
        .unwrap();
    panel.complete().await.unwrap();
    assert!(
        matches!(panel.model(), Some(Err(omega::Error::Effect(EffectError::Refused(refusal)))) if refusal == &Refusal::denied("no access"))
    );

    panel.send(Message::Insert).unwrap();
    panel
        .expect_storage_insert::<Tasks>()
        .await
        .unwrap()
        .fail(EffectError::Timeout)
        .unwrap();
    panel.complete().await.unwrap();
    assert!(matches!(
        panel.model(),
        Some(Err(omega::Error::Effect(EffectError::Timeout)))
    ));

    panel.send(Message::Replace(Fixture::revision(1))).unwrap();
    drop(panel.expect_storage_replace::<Tasks>().await.unwrap());
    panel.complete().await.unwrap();
    Fixture::closed(&panel);
}

#[tokio::test]
async fn invalid_success_tokens_are_rejected_instead_of_acknowledged() {
    let mut panel = Fixture::editor();
    for token in [
        Fixture::revision(0),
        Revision {
            epoch: "bad".into(),
            revision: 1,
        },
    ] {
        panel.send(Message::Insert).unwrap();
        assert!(
            panel
                .expect_storage_insert::<Tasks>()
                .await
                .unwrap()
                .succeed(token)
                .is_err()
        );
        panel.complete().await.unwrap();
        Fixture::closed(&panel);
    }
    for token in [
        Fixture::revision(1),
        Revision {
            epoch: "b".repeat(32),
            revision: 3,
        },
    ] {
        panel.send(Message::Replace(Fixture::revision(2))).unwrap();
        assert!(
            panel
                .expect_storage_replace::<Tasks>()
                .await
                .unwrap()
                .succeed(token)
                .is_err()
        );
        panel.complete().await.unwrap();
        Fixture::closed(&panel);
    }
    panel.send(Message::Remove(Fixture::revision(2))).unwrap();
    assert!(
        panel
            .expect_storage_remove::<Tasks>()
            .await
            .unwrap()
            .succeed(Fixture::revision(2))
            .is_err()
    );
    panel.complete().await.unwrap();
    Fixture::closed(&panel);
}

struct NumericValues;
impl Storage for NumericValues {
    type Key = String;
    type Value = u64;
    const ID: &'static str = Tasks::ID;
    const POLICY: StoragePolicy = StoragePolicy::Memory;
}

struct NumericKeys;
impl Storage for NumericKeys {
    type Key = u64;
    type Value = String;
    const ID: &'static str = Tasks::ID;
    const POLICY: StoragePolicy = StoragePolicy::Memory;
}

#[tokio::test]
async fn captures_decode_keys_and_values_through_the_selected_storage_contract() {
    let mut panel = Fixture::editor();
    panel.send(Message::Insert).unwrap();
    let error = panel
        .expect_storage_insert::<NumericValues>()
        .await
        .unwrap_err();
    assert!(error.to_string().contains("invalid captured storage value"));
    panel.complete().await.unwrap();
    Fixture::closed(&panel);

    panel.send(Message::Read("one".into())).unwrap();
    let error = panel
        .expect_storage_read::<NumericKeys>()
        .await
        .unwrap_err();
    assert!(error.to_string().contains("invalid key for example.tasks"));
    panel.complete().await.unwrap();
    Fixture::closed(&panel);
}
