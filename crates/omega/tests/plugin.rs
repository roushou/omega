//! What a plugin author writes, and what it costs them.

use omega::testing::{Called, Drawn, State, TestDaemon, manifest_of};
use omega::{
    Answer, Args, Battery, Bind, Button, Clock, Command, Field, Fields, Graph, Grid, Group, Header,
    Icon, Image, List, Network, Notify, Own, Percent, Progress, Row, Separator, Session, Slider,
    Spacer, Text, Toggle, Topic, Ui, Values, Watch, Widget,
};
use omega_proto::SystemTopic;
use omega_proto::omega::{Lock, action, value};

// ---- the shortest plugin anyone will write ----

#[derive(omega::Widget)]
struct Charge {
    battery: Battery,
}

impl Widget for Charge {
    fn render(&self) -> Ui {
        if self.battery.is_charging() {
            Text::new(format!("{} charging", self.battery.charge()))
        } else {
            Text::new(self.battery.charge())
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
        if self.battery.charge() < self.settings.low_threshold {
            Text::new("low").color("urgent")
        } else {
            Text::new(self.battery.charge())
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
            if charge < self.settings.low_threshold {
                Text::new(format!("{} low", self.settings.label))
            } else {
                Text::new(format!("{} {charge}", self.settings.label))
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

/// The shape an author writes once a topic can report nothing.
#[derive(omega::Widget)]
struct MaybeCharge {
    battery: Battery,
}

impl Widget for MaybeCharge {
    fn render(&self) -> Ui {
        if self.battery.has_reading() {
            Text::new(self.battery.charge())
        } else {
            Text::new("no battery")
        }
        .into()
    }
}

#[tokio::test]
async fn a_topic_with_nothing_to_report_is_an_answer_not_a_wait() {
    let mut daemon =
        TestDaemon::serving(omega::Plugin::named("charge", "0.1.0").widget::<MaybeCharge>());

    // A desktop has no battery, and the daemon says so by publishing the
    // topic with no value. That is an answer, so the widget draws — where
    // waiting for a reading that never comes held the first render forever.
    daemon
        .welcome(&State::new().absent(SystemTopic::Battery))
        .await;

    assert_eq!(daemon.next_view().await.view.text(), "no battery");
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
        if self.mode.get().focus {
            Text::new("focus").into()
        } else {
            Text::new("open").into()
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
            .child(Button::new("Connect").on_press(Bind::call("connect").arg("home")))
            .child(Slider::new(Percent::whole(60)).on_change(Bind::call("set").arg("output")))
            .child(Toggle::new(true).on_change("mute"))
            .child(
                Field::new("Passphrase")
                    .secret()
                    .on_submit(Bind::call("connect").arg("home")),
            )
            .child(
                List::new()
                    .child(Text::new("home").key("home"))
                    .on_activate("select"),
            )
            .child(Header::new("Networks"))
            .child(Separator::new())
            .child(Spacer::new().width(8))
            .child(Button::new("Forget").on_press("forget").disabled())
            .child(Button::new("Connecting").on_press("cancel").busy())
            .child(Graph::new(vec![14.0, 19.0, 12.0]).range(0.0, 100.0))
            .child(
                Group::new()
                    .option(Text::new("Auto").key("auto"))
                    .option(Text::new("5 GHz").key("5"))
                    .selected("auto")
                    .on_select("band"),
            )
            .child(Grid::new(2).gap(4).child(Text::new("Sent")))
            .child(Image::new("/tmp/art.png"))
            .child(Image::new("https://example.invalid/art.png"))
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
    // What a node *does* is not a prop. A binding lives beside them, so the
    // shell reads behaviour from one place rather than sniffing prop names.
    assert_eq!(children[3]["events"]["press"]["command"], "toggle");

    // Arguments travel as protobuf JSON `Value`s, which is exactly what an
    // `InvokeUnit` carries — so the shell forwards them verbatim instead of
    // re-encoding them, and this is the shape it forwards.
    let connect = &children[4]["events"]["press"];
    assert_eq!(connect["command"], "connect");
    assert_eq!(connect["args"][0]["stringValue"], "home");

    // A press with nothing to say carries no arguments at all.
    assert!(children[3]["events"]["press"].get("args").is_none());

    // A control reports; a display does not. Both carry their reading the
    // same way, and what separates them is whether anything is bound.
    assert_eq!(children[5]["type"], "slider");
    assert_eq!(children[5]["props"]["value"]["doubleValue"], 0.6);
    assert_eq!(children[5]["events"]["change"]["command"], "set");

    assert_eq!(children[6]["type"], "toggle");
    assert_eq!(children[6]["props"]["on"]["boolValue"], true);
    assert_eq!(children[6]["events"]["change"]["command"], "mute");

    // The value the user lands on is appended to these by the shell, so a
    // unit reads the arguments it chose by position and the reading last.
    assert_eq!(
        children[5]["events"]["change"]["args"][0]["stringValue"],
        "output"
    );

    // A field says how it draws, not what it holds: the buffer is the
    // shell's until the user commits it.
    assert_eq!(children[7]["type"], "field");
    assert_eq!(
        children[7]["props"]["placeholder"]["stringValue"],
        "Passphrase"
    );
    assert_eq!(children[7]["props"]["secret"]["boolValue"], true);
    assert_eq!(children[7]["events"]["submit"]["command"], "connect");

    // A list reports which row was activated, and the row's key is the
    // identity it reports — so a row must carry one.
    assert_eq!(children[8]["type"], "list");
    assert_eq!(children[8]["events"]["activate"]["command"], "select");
    assert_eq!(children[8]["children"][0]["key"], "home");

    assert_eq!(children[9]["type"], "header");
    assert_eq!(children[9]["props"]["text"]["stringValue"], "Networks");
    assert_eq!(children[10]["type"], "separator");
    assert_eq!(children[11]["type"], "spacer");
    assert_eq!(children[11]["props"]["width"]["intValue"], "8");

    // Two reasons a control cannot be used, and the shell is told which:
    // one is waiting on an answer and the other is simply not available.
    assert_eq!(children[12]["props"]["disabled"]["boolValue"], true);
    assert_eq!(children[13]["props"]["busy"]["boolValue"], true);

    // The one prop that is not a single value. `Value` has carried a list all
    // along; nothing needed one until something had to draw a series.
    let graph = &children[14];
    assert_eq!(graph["type"], "graph");
    assert_eq!(
        graph["props"]["points"]["list"]["values"][0]["doubleValue"],
        14.0
    );
    assert_eq!(
        graph["props"]["points"]["list"]["values"][2]["doubleValue"],
        12.0
    );
    // Pinned, because a percentage that scaled to its own noise would show an
    // idle machine as one on fire.
    assert_eq!(graph["props"]["high"]["doubleValue"], 100.0);

    assert_eq!(children[15]["type"], "group");
    assert_eq!(children[15]["props"]["selected"]["stringValue"], "auto");
    assert_eq!(children[15]["children"][1]["key"], "5");

    assert_eq!(children[16]["type"], "grid");
    assert_eq!(children[16]["props"]["columns"]["intValue"], "2");

    // A local file reaches the shell; a URL does not. Fetching what a unit
    // named would make the shell issue requests on its behalf, which no
    // capability granted — so the source is dropped and the node draws
    // nothing rather than reaching out.
    assert_eq!(
        children[17]["props"]["source"]["stringValue"],
        "/tmp/art.png"
    );
    assert!(children[18]["props"].get("source").is_none());

    // Keys are the path to a node, and the renderer keeps a node whose key it
    // already has rather than rebuilding it.
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

// ---- publishing a collection ----

/// One access point, as a unit that scanned for them would report it.
#[derive(omega::Config, Debug, Clone, Default, PartialEq)]
struct AccessPoint {
    ssid: String,
    signal: u32,
    secured: bool,
}

/// A unit's own state, which is a list — the shape most units actually have
/// to publish, and the one that did not compile until `Vec<T>` was a value.
#[derive(omega::Topic, Debug, Clone, Default, PartialEq)]
struct Scan {
    found: Vec<AccessPoint>,
    names: Vec<String>,
}

#[test]
fn a_unit_can_publish_a_list() {
    let scan = Scan {
        found: vec![
            AccessPoint {
                ssid: "home".into(),
                signal: 70,
                secured: true,
            },
            AccessPoint {
                ssid: "cafe".into(),
                signal: 30,
                secured: false,
            },
        ],
        names: vec!["home".into(), "cafe".into()],
    };

    // Through the same map a keyspace value travels as. A struct in the list
    // is a value in its own right, which is what lets it be in one.
    let round_tripped = Scan::read(&scan.write());
    assert_eq!(round_tripped, scan);
}

#[test]
fn a_list_that_cannot_be_read_takes_the_default() {
    // Reading a *field* is total, so a list of the wrong shape reads as the
    // type's default rather than failing the whole document — the same rule
    // that lets a field be added without breaking a writer that predates it.
    let wrong = Values::new().with("names", 7_i64);
    assert_eq!(Scan::read(&wrong).names, Vec::<String>::new());
}

#[test]
fn one_unreadable_element_is_not_a_shorter_list() {
    // All or nothing on the way back. Dropping the element nobody could read
    // would hand back a list that looks complete and is not — which is worse
    // than saying the list was not understood.
    let mixed: Vec<omega::internal::Value> = vec![
        omega::internal::IntoValue::into_value("home"),
        omega::internal::IntoValue::into_value(7_i64),
    ];
    let read: Option<Vec<String>> =
        omega::internal::FromValue::from_value(&omega::internal::IntoValue::into_value(mixed));
    assert!(read.is_none());
}

// ---- the clock ----

#[derive(omega::Widget)]
struct BarClock {
    clock: Clock,
}

impl Widget for BarClock {
    fn render(&self) -> Ui {
        Text::new(format!(
            "{} {}",
            self.clock.weekday().short(),
            self.clock.time()
        ))
        .into()
    }
}

#[test]
fn a_clock_draws_the_time_without_carrying_a_calendar() {
    // The daemon applies the zone rules and hands over the parts. A unit that
    // had to convert a timestamp would need the whole tz database to draw two
    // digits.
    let state = State::new().with(omega_proto::omega::TimeState {
        year: 2026,
        month: 9,
        day: 8,
        hour: 14,
        minute: 32,
        weekday: 2,
        zone: "CEST".into(),
        utc_offset_seconds: 7200,
        unix_seconds: 1_788_957_120,
    });

    assert_eq!(Drawn::of::<BarClock>(&state).text(), "Tue 14:32");
}

#[test]
fn a_clock_with_no_reading_draws_a_time_rather_than_a_panic() {
    // Every accessor falls back, so a widget built before the first tick
    // draws something wrong rather than taking the unit down.
    assert_eq!(Drawn::of::<BarClock>(&State::new()).text(), "Sun 00:00");
}

// ---- a topic nobody wrote accessors for ----

/// Reading a topic that has no typed accessors, only the floor `get()` gives.
#[derive(omega::Widget)]
struct Devices {
    bluetooth: omega::Bluetooth,
}

impl Widget for Devices {
    fn render(&self) -> Ui {
        let Some(state) = self.bluetooth.get() else {
            return Ui::empty();
        };
        Text::new(format!("{} paired", state.devices.len())).into()
    }
}

#[test]
fn a_topic_with_no_accessors_is_still_readable() {
    // The point of generating a handle for every topic: `bluetooth` was
    // brokered, coalesced and replicated for a dozen commits with no way for
    // a unit to name it. Nobody has written `paired()` or `is_connected()`
    // yet, and it is readable anyway.
    let state = State::new().with(omega_proto::omega::BluetoothState {
        available: true,
        powered: true,
        discovering: false,
        devices: vec![omega_proto::omega::BluetoothDevice {
            address: "60:AB:D2:25:8C:49".into(),
            name: "Bose NC 700".into(),
            connected: true,
            paired: true,
            icon: "audio-headphones".into(),
            battery_percent: 72,
        }],
    });

    assert_eq!(Drawn::of::<Devices>(&state).text(), "1 paired");
    assert!(Drawn::of::<Devices>(&State::new()).is_empty());
}

#[test]
fn a_handle_declares_the_topic_its_type_names() {
    // The manifest comes from the fields, so holding `Bluetooth` is what asks
    // for the topic — there is no string anywhere to get wrong.
    let manifest =
        omega::testing::manifest_of(&omega::Plugin::named("devices", "0.1.0").widget::<Devices>());
    assert_eq!(manifest.state_topics, vec!["bluetooth"]);
}
