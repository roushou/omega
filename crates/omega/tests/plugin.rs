//! What a plugin author writes, and what it costs them.

use omega::testing::{Called, Drawn, State, TestDaemon, manifest_of};
use omega::{
    Answer, Args, Battery, Button, Command, Fields, Icon, Network, Notify, Own, Percent, Progress,
    Row, Session, Text, Topic, Ui, Values, Watch, Widget,
};
use omega_proto::omega::{Lock, action, value};

// ---- the shortest plugin anyone will write ----

#[derive(omega::Widget)]
struct Charge {
    battery: Battery,
}

impl Widget for Charge {
    fn render(&self) -> Ui {
        match self.battery.is_charging() {
            true => Text::new(format!("{} charging", self.battery.charge())),
            false => Text::new(self.battery.charge()),
        }
        .into()
    }
}

#[test]
fn a_widget_is_a_function_from_state_to_a_tree() {
    let drawn = Drawn::of::<Charge>(&State::new().battery(0.8, false));

    assert_eq!(drawn.text(), "80%");
    assert_eq!(drawn.kinds(), vec!["text"]);
    // One node needs no tree to be wrapped in, and no key to be given.
    assert_eq!(drawn.keys(), vec!["root"]);
}

#[test]
fn a_reading_prints_itself() {
    // The wire carries 0.0 .. 1.0 and a bar shows "80%". A plugin that had to
    // multiply by a hundred is a plugin that can forget to.
    assert_eq!(Percent::of(0.8).to_string(), "80%");
    assert_eq!(Percent::whole(80), Percent::of(0.8));
    assert!(Percent::of(0.15) < 20);

    let drawn = Drawn::of::<Charge>(&State::new().battery(0.5, true));
    assert_eq!(drawn.text(), "50% charging");
}

#[test]
fn a_plugins_manifest_is_the_sum_of_its_fields() {
    let manifest = manifest_of(&omega::Plugin::named("charge", "0.1.0").widget::<Charge>());

    // Nothing here was typed by an author. Holding a `Battery` is what asks
    // to read the battery topic, and asking to read is what costs the
    // capability.
    assert_eq!(manifest.state_topics, vec!["battery"]);
    assert_eq!(manifest.capabilities, vec!["CAPABILITY_STATE_READ"]);
    assert_eq!(manifest.surfaces.len(), 1);
    assert_eq!(manifest.surfaces[0].id.as_str(), "charge");
    assert_eq!(manifest.surfaces[0].kind, "SURFACE_KIND_WIDGET");
    assert_eq!(manifest.name.as_str(), "charge");
}

// ---- composition ----

#[derive(omega::Widget)]
struct Signal {
    network: Network,
}

impl Widget for Signal {
    fn render(&self) -> Ui {
        Row::new()
            .gap(6)
            .child(Icon::new("wifi"))
            .child(Text::new(self.network.ssid().unwrap_or_default()))
            .child(Progress::new(self.network.strength()))
            .into()
    }
}

#[test]
fn a_view_is_built_out_of_the_things_it_is_made_of() {
    let drawn = Drawn::of::<Signal>(&State::new().network("home", 70));

    assert_eq!(drawn.text(), "home");
    assert_eq!(drawn.kinds(), vec!["stack", "icon", "text", "progress"]);
    // Keys are the path to a node, so a tree that did not change shape keeps
    // the identities the reconciler diffs against.
    assert_eq!(drawn.keys(), vec!["root", "root.0", "root.1", "root.2"]);
}

#[derive(omega::Command)]
struct LockScreen {
    session: Session,
}

impl Command for LockScreen {
    fn call(&self, _args: Args) -> Answer {
        self.session.lock();
        Answer::done()
    }
}

#[test]
fn a_plugin_can_draw_and_do_at_once() {
    let manifest = manifest_of(
        &omega::Plugin::named("battery", "0.1.0")
            .widget::<Charge>()
            .command::<LockScreen>("lock"),
    );

    // Two surfaces, of two kinds, from one plugin — and the union of what
    // both need.
    let kinds: Vec<&str> = manifest
        .surfaces
        .iter()
        .map(|surface| surface.kind.as_str())
        .collect();
    assert_eq!(kinds, vec!["SURFACE_KIND_WIDGET", "SURFACE_KIND_COMMAND"]);
    assert_eq!(
        manifest.capabilities,
        vec!["CAPABILITY_STATE_READ", "CAPABILITY_SYSTEM_CONTROL"]
    );
}

#[test]
fn a_command_is_judged_by_what_it_asked_the_machine_to_do() {
    let called = Called::of::<LockScreen>(&State::new(), Vec::new());

    assert!(matches!(called.answer, Answer::Done));
    assert!(called.did(&action::Kind::Lock(Lock {})));
}

// ---- configuration ----

#[derive(omega::Config, Default, Debug, Clone, PartialEq)]
struct Warning {
    low_threshold: u8,
}

#[derive(omega::Widget)]
struct Warned {
    battery: Battery,
    #[omega(config)]
    settings: Warning,
}

impl Widget for Warned {
    fn render(&self) -> Ui {
        match self.battery.charge() < self.settings.low_threshold {
            true => Text::new("low").color("urgent"),
            false => Text::new(self.battery.charge()),
        }
        .into()
    }
}

#[test]
fn an_instance_is_configured_by_the_document() {
    let state = State::new().battery(0.15, false);

    // A setting the document did not set falls back to the field's default,
    // so adding one never breaks a document written before it existed.
    assert_eq!(Drawn::of::<Warned>(&state).text(), "15%");

    // Written by the same fields that read it: `low_threshold` in Rust is
    // `low-threshold` in a document, decided once by the derive.
    let settings = Warning { low_threshold: 20 }.write();
    assert_eq!(settings.get::<u8>("low-threshold"), Some(20));
    assert_eq!(Drawn::configured::<Warned>(&state, &settings).text(), "low");

    // And read back into the same value it was written from.
    assert_eq!(Warning::read(&settings), Warning { low_threshold: 20 });
}

/// A command with settings.
///
/// The case a placement cannot serve: a command is never put in a bar, so the
/// settings its unit was given are the only ones it can ever have.
#[derive(omega::Command)]
struct Threshold {
    #[omega(config)]
    settings: Warning,
}

impl Command for Threshold {
    fn call(&self, _args: Args) -> Answer {
        Answer::value(self.settings.low_threshold.to_string())
    }
}

/// What a command handed back, when it handed back text.
fn answered(answer: &Answer) -> Option<String> {
    match answer {
        Answer::Value(value) => match value.kind.as_ref()? {
            value::Kind::StringValue(text) => Some(text.clone()),
            _ => None,
        },
        _ => None,
    }
}

#[test]
fn a_command_is_configured_by_its_unit() {
    let state = State::new();
    let settings = Warning { low_threshold: 20 }.write();

    let configured = Called::configured::<Threshold>(&state, &settings, Vec::new());
    assert_eq!(answered(&configured.answer).as_deref(), Some("20"));

    // ...and falls back to the field's default when the document said nothing.
    let bare = Called::of::<Threshold>(&state, Vec::new());
    assert_eq!(answered(&bare.answer).as_deref(), Some("0"));
}

#[tokio::test]
async fn a_unit_is_told_its_settings_at_the_handshake() {
    let mut daemon = TestDaemon::serving(
        omega::Plugin::named("battery", "0.1.0").command::<Threshold>("threshold"),
    );

    // There is no later moment that would do: a plugin's fields are built out
    // of its settings, so a unit that was not told them at construction was
    // built without them.
    daemon
        .welcome_configured(&State::new(), &Warning { low_threshold: 20 }.write())
        .await;

    let answer = daemon
        .call("threshold", Vec::new())
        .await
        .expect("the command answered")
        .expect("...with a value");
    assert_eq!(
        answer.kind,
        Some(value::Kind::StringValue("20".to_string()))
    );
}

#[tokio::test]
async fn where_a_widget_is_placed_adds_to_how_its_unit_was_configured() {
    #[derive(omega::Config, Default, Debug, Clone, PartialEq)]
    struct Look {
        low_threshold: u8,
        label: String,
    }

    #[derive(omega::Widget)]
    struct Labelled {
        battery: Battery,
        #[omega(config)]
        settings: Look,
    }

    impl Widget for Labelled {
        fn render(&self) -> Ui {
            let charge = self.battery.charge();
            match charge < self.settings.low_threshold {
                true => Text::new(format!("{} low", self.settings.label)),
                false => Text::new(format!("{} {charge}", self.settings.label)),
            }
            .into()
        }
    }

    let mut daemon =
        TestDaemon::serving(omega::Plugin::named("battery", "0.1.0").widget::<Labelled>());

    let unit = Look {
        low_threshold: 20,
        label: "batt".to_string(),
    }
    .write();
    daemon
        .welcome_configured(&State::new().battery(0.15, false), &unit)
        .await;

    // Before it is placed anywhere, a widget draws with its unit's settings.
    assert_eq!(daemon.next_view().await.view.text(), "batt low");

    // A placement that names one key must not silently reset the others: the
    // threshold it says nothing about is still the unit's 20, not the
    // field's default of 0.
    let drawn = daemon
        .render(
            "battery",
            "top-bar-1",
            Values::new().with("label", "power").into_map(),
        )
        .await;
    assert_eq!(drawn.text(), "power low");
}

// ---- the protocol, when a test needs it ----

#[tokio::test]
async fn a_plugin_publishes_its_view_and_keeps_publishing() {
    let mut daemon =
        TestDaemon::serving(omega::Plugin::named("charge", "0.1.0").widget::<Charge>());

    let hello = daemon.welcome(&State::new().battery(0.5, true)).await;
    assert_eq!(
        hello.manifest_hash,
        manifest_of(&omega::Plugin::named("charge", "0.1.0").widget::<Charge>()).hash(),
        "a plugin presents the hash of the manifest it derived from itself"
    );

    assert_eq!(daemon.next_view().await.view.text(), "50% charging");

    daemon.publish(&State::new().battery(0.2, false)).await;
    assert_eq!(daemon.next_view().await.view.text(), "20%");
}

#[tokio::test]
async fn a_widget_is_not_asked_to_draw_a_machine_it_cannot_see() {
    let mut daemon =
        TestDaemon::serving(omega::Plugin::named("charge", "0.1.0").widget::<Charge>());

    // No battery in the snapshot: a widget that declared one has nothing
    // truthful to draw, so it is not asked to.
    daemon.welcome(&State::new()).await;
    daemon.publish(&State::new().battery(0.42, false)).await;

    // The first view it ever publishes is of a real reading.
    assert_eq!(daemon.next_view().await.view.text(), "42%");
}

#[tokio::test]
async fn a_document_can_instantiate_one_surface_more_than_once() {
    let mut daemon =
        TestDaemon::serving(omega::Plugin::named("warned", "0.1.0").widget_as::<Warned>("warned"));
    daemon.welcome(&State::new().battery(0.15, false)).await;
    let _first = daemon.next_view().await;

    let strict = Warning { low_threshold: 20 }.write().into_map();

    // Each instance is its own value with its own settings, so there is no
    // "which instance am I" for a plugin to ask.
    assert_eq!(
        daemon.render("warned", "top-bar-1", strict).await.text(),
        "low"
    );
    assert_eq!(
        daemon
            .render("warned", "top-bar-2", Default::default())
            .await
            .text(),
        "15%"
    );
}

#[derive(omega::Command)]
struct Announce {
    notify: Notify,
}

impl Command for Announce {
    fn call(&self, args: Args) -> Answer {
        let Some(text) = args.get::<String>(0) else {
            return Answer::refused("announce takes one string");
        };
        self.notify.send(text.clone());
        Answer::value(text)
    }
}

#[tokio::test]
async fn a_command_answers_over_the_wire() {
    let mut daemon =
        TestDaemon::serving(omega::Plugin::named("announce", "0.1.0").command::<Announce>("say"));
    daemon.welcome(&State::new()).await;

    let said = daemon
        .call(
            "say",
            vec![omega_proto::omega::Value {
                kind: Some(value::Kind::StringValue("hello".into())),
            }],
        )
        .await;
    assert_eq!(
        said,
        Ok(Some(omega_proto::omega::Value {
            kind: Some(value::Kind::StringValue("hello".into()))
        }))
    );

    // A refusal is the plugin's own words, and a test can hold it to them.
    assert_eq!(
        daemon.call("say", Vec::new()).await,
        Err("announce takes one string".to_string())
    );
    assert_eq!(
        daemon.call("shout", Vec::new()).await,
        Err("no command shout".to_string())
    );
}

// ---- state a plugin owns, and another plugin reads ----

#[derive(omega::Topic, Default, Debug, Clone, PartialEq)]
struct Mode {
    focus: bool,
}

#[derive(omega::Command)]
struct Focus {
    mode: Own<Mode>,
}

impl Command for Focus {
    fn call(&self, args: Args) -> Answer {
        let on = args.get::<bool>(0).unwrap_or(true);
        self.mode.set(&Mode { focus: on });
        Answer::done()
    }
}

#[derive(omega::Widget)]
struct Showing {
    mode: Watch<Mode>,
}

impl Widget for Showing {
    fn render(&self) -> Ui {
        match self.mode.get().focus {
            true => Text::new("focus").into(),
            false => Text::new("open").into(),
        }
    }
}

#[test]
fn a_topics_address_comes_from_where_it_is_defined() {
    // The crate that defines the type owns the keyspace, and the type names
    // the key. Neither is a string anybody typed — including here: asserting
    // the literal would pin this crate's own package name, which is not what
    // the rule is about.
    assert_eq!(Mode::UNIT, env!("CARGO_PKG_NAME"));
    assert_eq!(Mode::KEY, "mode");
    assert_eq!(Mode::address(), format!("unit.{}.mode", Mode::UNIT));
}

#[test]
fn owning_state_declares_the_right_to_publish_it() {
    let manifest = manifest_of(
        &omega::Plugin::named("desk", "0.1.0")
            .widget::<Showing>()
            .command::<Focus>("focus"),
    );

    // Writing needs permission to write; reading somebody's keyspace is what
    // a manifest declares, so both ends of this plugin are in it.
    assert_eq!(
        manifest.capabilities,
        vec!["CAPABILITY_STATE_READ", "CAPABILITY_STATE_WRITE"]
    );
    assert_eq!(manifest.state_topics, vec![Mode::address()]);
}

#[test]
fn a_widget_reads_state_that_has_never_been_set() {
    // Nobody has published a mode, so the type's own default is what a
    // reader sees — there is no half-built state to guard against.
    assert_eq!(Drawn::of::<Showing>(&State::new()).text(), "open");
}

#[test]
fn publishing_state_is_an_effect_like_any_other() {
    let called = Called::of::<Focus>(&State::new(), Vec::new());

    // A plugin does not write its own keyspace directly: it asks the daemon,
    // which owns the value from there on and replicates it to whoever reads.
    let published = called
        .effects
        .iter()
        .find_map(|op| match op {
            omega_proto::omega::invoke::Op::SetState(set) => Some(set),
            _ => None,
        })
        .expect("focus publishes the mode");

    assert_eq!(published.topic, Mode::address());
    assert_eq!(
        Mode::read(
            &<omega::Values as omega::FromValue>::from_value(published.value.as_ref().unwrap())
                .unwrap()
        ),
        Mode { focus: true }
    );
}

#[tokio::test]
async fn one_plugin_draws_what_another_published() {
    let mut daemon = TestDaemon::serving(
        omega::Plugin::named("desk", "0.1.0")
            .widget::<Showing>()
            .command::<Focus>("focus"),
    );
    daemon.welcome(&State::new()).await;
    assert_eq!(daemon.next_view().await.view.text(), "open");

    // The daemon replicates a keyspace like any other topic, so a widget
    // watching one re-renders when it moves — whoever moved it.
    daemon
        .publish(&State::new().keyspace(&Mode::address(), Mode { focus: true }.write()))
        .await;
    assert_eq!(daemon.next_view().await.view.text(), "focus");
}

// ---- the contract the shell reads ----

/// What a node looks like on the wire, as the renderer sees it.
///
/// The shell is QML and cannot be compiled against these types, so this is
/// where the two halves are held to the same shape: a prop renamed here and
/// not there is a widget that silently draws nothing.
fn wire(ui: Ui) -> serde_json::Value {
    serde_json::to_value(ui.into_tree()).unwrap()
}

#[test]
fn every_node_kind_carries_the_props_the_renderer_reads() {
    let tree = wire(
        Row::new()
            .gap(6)
            .child(Text::new("80%").bold().color("urgent"))
            .child(Icon::new("battery"))
            .child(Progress::new(Percent::of(0.7)))
            .child(Button::new("toggle").on_press("toggle"))
            .into(),
    );

    let root = &tree["root"];
    assert_eq!(root["type"], "stack");
    assert_eq!(root["props"]["align"]["stringValue"], "row");
    // Protobuf JSON writes a 64-bit integer as a string, which a reader has
    // to parse rather than use. Pinned here because forgetting it renders a
    // gap of NaN.
    assert_eq!(root["props"]["gap"]["intValue"], "6");

    let children = root["children"].as_array().unwrap();
    assert_eq!(children[0]["type"], "text");
    assert_eq!(children[0]["props"]["text"]["stringValue"], "80%");
    assert_eq!(children[0]["props"]["bold"]["boolValue"], true);
    assert_eq!(children[0]["props"]["color"]["stringValue"], "urgent");

    assert_eq!(children[1]["type"], "icon");
    assert_eq!(children[1]["props"]["name"]["stringValue"], "battery");

    assert_eq!(children[2]["type"], "progress");
    assert_eq!(children[2]["props"]["value"]["doubleValue"], 0.7);

    assert_eq!(children[3]["type"], "button");
    assert_eq!(children[3]["props"]["label"]["stringValue"], "toggle");
    assert_eq!(children[3]["props"]["command"]["stringValue"], "toggle");

    // Keys are the path to a node, and the renderer diffs on them.
    assert_eq!(root["key"], "root");
    assert_eq!(children[3]["key"], "root.3");
}

#[test]
fn a_widget_that_draws_nothing_says_so() {
    // An empty view has no root, which the shell renders as absent rather
    // than as a gap where something used to be.
    assert_eq!(wire(Ui::empty())["root"], serde_json::Value::Null);
}

#[tokio::test]
async fn an_instantiated_surface_stops_publishing_anonymously() {
    let mut daemon =
        TestDaemon::serving(omega::Plugin::named("charge", "0.1.0").widget::<Charge>());
    daemon.welcome(&State::new().battery(0.5, false)).await;

    // Until the document says otherwise a surface has one instance, and a
    // plugin publishes to it.
    let first = daemon.next_view().await;
    assert_eq!(first.module, "");

    // Being handed an instance is being told otherwise. Everything after
    // belongs to that instance — a plugin that kept publishing anonymously
    // would leave the document's instance frozen at whatever it last
    // answered, which is a widget that looks alive and reports a stale
    // number.
    daemon
        .render("charge", "top-bar-1", Default::default())
        .await;

    daemon.publish(&State::new().battery(0.2, false)).await;
    let after = daemon.next_view().await;
    assert_eq!(after.module, "top-bar-1", "published to the wrong instance");
    assert_eq!(after.view.text(), "20%");
}
