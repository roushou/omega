//! What a plugin author writes, and what it costs them.

use omega::config::{Fields, Values};
use omega::platform::network::Network;
use omega::platform::notification::Notify;
use omega::platform::power::Battery;
use omega::platform::session::Session;
use omega::platform::time::Clock;
use omega::record::{Own, PluginState, Watch};
use omega::testing::{Called, Drawn, State, TestDaemon, manifest_of};
use omega::ui::{
    Button, Choice, Field, Glyph, Graph, Grid, Header, Icon, Image, List, Progress, Row, Separator,
    Slider, Spacer, Text, Toggle,
};
use omega::{Args, Command, Percent, Surface, Ui};
use omega_proto::SystemTopic;
use omega_proto::omega::{Capability, Lock, SurfaceKind, action, value};

// ---- the shortest plugin anyone will write ----

#[derive(omega::Surface)]
struct Charge {
    battery: Battery,
}

impl Surface for Charge {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();
    fn update(
        &self,
        _: &mut (),
        message: Self::Message,
        _: &(),
    ) -> omega::surface::Task<Self::Message> {
        match message {}
    }
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> Ui {
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
    let drawn = Drawn::of::<Charge>(&State::new().battery(0.8, false)).unwrap();

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

    let drawn = Drawn::of::<Charge>(&State::new().battery(0.5, true)).unwrap();
    assert_eq!(drawn.text(), "50% charging");
}

#[test]
fn a_plugins_manifest_is_the_sum_of_its_fields() {
    let manifest =
        manifest_of(&omega::Plugin::named("charge", "0.1.0").surface_default::<Charge>());

    // Reading fields declare their topic subscriptions and capabilities.
    assert_eq!(manifest.state_topics, vec!["battery"]);
    assert_eq!(manifest.granted().unwrap(), vec![Capability::StateRead]);
    assert_eq!(manifest.surfaces.len(), 1);
    assert_eq!(manifest.surfaces[0].id.as_str(), "charge");
    assert_eq!(
        manifest.surfaces[0].declared().unwrap(),
        SurfaceKind::Widget
    );
    assert_eq!(manifest.name.as_str(), "charge");
}

// ---- composition ----

#[derive(omega::Surface)]
struct Signal {
    network: Network,
}

impl Surface for Signal {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();
    fn update(
        &self,
        _: &mut (),
        message: Self::Message,
        _: &(),
    ) -> omega::surface::Task<Self::Message> {
        match message {}
    }
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> Ui {
        Row::new()
            .gap(6)
            .child(Icon::new(Glyph::Wifi))
            .child(Text::new(self.network.ssid().unwrap_or_default()))
            .child(Progress::new(self.network.strength()))
            .into()
    }
}

#[test]
fn a_view_is_built_out_of_the_things_it_is_made_of() {
    let drawn = Drawn::of::<Signal>(&State::new().network("home", 70)).unwrap();

    assert_eq!(drawn.text(), "home");
    assert_eq!(drawn.kinds(), vec!["stack", "icon", "text", "progress"]);
    // Keys are the path to a node, so a tree that did not change shape keeps
    // the identities the reconciler diffs against.
    assert_eq!(drawn.keys(), vec!["root", "root.0", "root.1", "root.2"]);
}

#[derive(omega::Command)]
#[omega(name = "lock")]
struct LockScreen {
    session: Session,
}

impl Command for LockScreen {
    type Input = Args;
    type Output = ();
    async fn call(&self, _args: Args) -> Result<Self::Output, omega::Error> {
        self.session.lock().await
    }
}

#[test]
fn a_plugin_can_draw_and_do_at_once() {
    let manifest = manifest_of(
        &omega::Plugin::named("battery", "0.1.0")
            .surface_default::<Charge>()
            .command::<LockScreen>(),
    );

    assert_eq!(manifest.surface_kinds().unwrap(), vec![SurfaceKind::Widget]);
    assert_eq!(
        manifest.granted().unwrap(),
        vec![Capability::StateRead, Capability::SystemControl]
    );
}

#[tokio::test]
async fn a_command_is_judged_by_what_it_asked_the_machine_to_do() {
    let called = Called::raw::<LockScreen>(&State::new(), Vec::new()).await;

    assert!(called.answer.is_ok());
    assert!(called.did(&action::Kind::Lock(Lock {})));
}

// ---- configuration ----

#[derive(omega::Config, Default, Debug, Clone, PartialEq)]
struct Warning {
    low_threshold: u8,
}

#[derive(omega::Surface)]
struct Warned {
    battery: Battery,
    #[omega(config)]
    settings: Warning,
}

impl Surface for Warned {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();
    fn update(
        &self,
        _: &mut (),
        message: Self::Message,
        _: &(),
    ) -> omega::surface::Task<Self::Message> {
        match message {}
    }
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> Ui {
        if self.battery.charge() < self.settings.low_threshold {
            Text::new("low").warning()
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
    assert_eq!(Drawn::of::<Warned>(&state).unwrap().text(), "15%");

    // Written by the same fields that read it: `low_threshold` in Rust is
    // `low-threshold` in a document, decided once by the derive.
    let settings = Warning { low_threshold: 20 }.write();
    assert_eq!(settings.get::<u8>("low-threshold"), Some(20));
    assert_eq!(
        Drawn::configured::<Warned>(&state, &settings)
            .unwrap()
            .text(),
        "low"
    );

    // And read back into the same value it was written from.
    assert_eq!(Warning::read(&settings), Warning { low_threshold: 20 });
}

/// Command fixture using plugin-level settings.
#[derive(omega::Command)]
#[omega(name = "threshold")]
struct Threshold {
    #[omega(config)]
    settings: Warning,
}

impl Command for Threshold {
    type Input = Args;
    type Output = String;
    async fn call(&self, _args: Args) -> Result<Self::Output, omega::Error> {
        Ok(self.settings.low_threshold.to_string())
    }
}

/// What a command handed back, when it handed back text.
fn answered(answer: &Result<String, omega::Error>) -> Option<String> {
    answer.as_ref().ok().cloned()
}

#[tokio::test]
async fn a_command_is_configured_by_its_plugin() {
    let state = State::new();
    let settings = Warning { low_threshold: 20 }.write();

    let configured = Called::configured::<Threshold>(&state, &settings, Vec::new()).await;
    assert_eq!(answered(&configured.answer).as_deref(), Some("20"));

    // ...and falls back to the field's default when the document said nothing.
    let bare = Called::raw::<Threshold>(&state, Vec::new()).await;
    assert_eq!(answered(&bare.answer).as_deref(), Some("0"));
}

#[tokio::test]
async fn a_plugin_is_told_its_settings_at_the_handshake() {
    let mut daemon =
        TestDaemon::serving(omega::Plugin::named("battery", "0.1.0").command::<Threshold>());

    // Settings must be available when plugin fields are constructed.
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
async fn where_a_widget_is_placed_adds_to_how_its_plugin_was_configured() {
    #[derive(omega::Config, Default, Debug, Clone, PartialEq)]
    struct Look {
        low_threshold: u8,
        label: String,
    }

    #[derive(omega::Surface)]
    struct Labelled {
        battery: Battery,
        #[omega(config)]
        settings: Look,
    }

    impl Surface for Labelled {
        type Model = ();
        type Message = std::convert::Infallible;
        type Effects = ();
        fn update(
            &self,
            _: &mut (),
            message: Self::Message,
            _: &(),
        ) -> omega::surface::Task<Self::Message> {
            match message {}
        }
        fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> Ui {
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
        TestDaemon::serving(omega::Plugin::named("battery", "0.1.0").surface_default::<Labelled>());

    let plugin = Look {
        low_threshold: 20,
        label: "batt".to_string(),
    }
    .write();
    daemon
        .welcome_configured(&State::new().battery(0.15, false), &plugin)
        .await;

    // Instance settings layer over the plugin settings.
    assert_eq!(
        daemon
            .render("battery", "test", Default::default())
            .await
            .text(),
        "batt low"
    );

    // Placement settings override only specified keys.
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
        TestDaemon::serving(omega::Plugin::named("charge", "0.1.0").surface_default::<Charge>());

    let hello = daemon.welcome(&State::new().battery(0.5, true)).await;
    assert_eq!(
        hello.manifest_hash,
        manifest_of(&omega::Plugin::named("charge", "0.1.0").surface_default::<Charge>()).hash(),
        "a plugin presents the hash of the manifest it derived from itself"
    );

    assert_eq!(
        daemon
            .render("charge", "test", Default::default())
            .await
            .text(),
        "50% charging"
    );

    daemon.publish(&State::new().battery(0.2, false)).await;
    assert_eq!(daemon.next_view().await.view.text(), "20%");
}

#[tokio::test]
async fn a_widget_is_not_asked_to_draw_a_machine_it_cannot_see() {
    let mut daemon =
        TestDaemon::serving(omega::Plugin::named("charge", "0.1.0").surface_default::<Charge>());

    // No battery in the snapshot: a widget that declared one has nothing
    // truthful to draw, so it is not asked to.
    daemon.welcome(&State::new()).await;
    let _pending = daemon.render("charge", "test", Default::default()).await;
    daemon.publish(&State::new().battery(0.42, false)).await;

    // The first view it ever publishes is of a real reading.
    assert_eq!(daemon.next_view().await.view.text(), "42%");
}

/// The shape an author writes once a topic can report nothing.
#[derive(omega::Surface)]
struct MaybeCharge {
    battery: Battery,
}

impl Surface for MaybeCharge {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();
    fn update(
        &self,
        _: &mut (),
        message: Self::Message,
        _: &(),
    ) -> omega::surface::Task<Self::Message> {
        match message {}
    }
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> Ui {
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
    let mut daemon = TestDaemon::serving(
        omega::Plugin::named("charge", "0.1.0").surface_default::<MaybeCharge>(),
    );

    // Reported absence satisfies readiness and allows the first render.
    daemon
        .welcome(&State::new().absent(SystemTopic::Battery))
        .await;

    assert_eq!(
        daemon
            .render("charge", "test", Default::default())
            .await
            .text(),
        "no battery"
    );
}

#[tokio::test]
async fn a_document_can_instantiate_one_surface_more_than_once() {
    let mut daemon =
        TestDaemon::serving(omega::Plugin::named("warned", "0.1.0").surface_as::<Warned>("warned"));
    daemon.welcome(&State::new().battery(0.15, false)).await;
    let _first = daemon.render("warned", "test", Default::default()).await;

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
#[omega(name = "say")]
struct Announce {
    notify: Notify,
}

impl Command for Announce {
    type Input = Args;
    type Output = String;
    async fn call(&self, args: Args) -> Result<Self::Output, omega::Error> {
        let Some(text) = args.get::<String>(0) else {
            return Err(omega::Error::invalid("announce takes one string"));
        };
        self.notify.send(text.clone()).await?;
        Ok(text)
    }
}

#[tokio::test]
async fn a_command_answers_over_the_wire() {
    let mut daemon =
        TestDaemon::serving(omega::Plugin::named("announce", "0.1.0").command::<Announce>());
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

#[derive(omega::PluginState, Default, Debug, Clone, PartialEq)]
struct Mode {
    focus: bool,
}

#[derive(omega::Command)]
#[omega(name = "focus")]
struct Focus {
    mode: Own<Mode>,
}

impl Command for Focus {
    type Input = Args;
    type Output = ();
    async fn call(&self, args: Args) -> Result<Self::Output, omega::Error> {
        let on = args.get::<bool>(0).unwrap_or(true);
        self.mode.set(&Mode { focus: on }).await
    }
}

#[derive(omega::Surface)]
struct Showing {
    mode: Watch<Mode>,
}

impl Surface for Showing {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();
    fn update(
        &self,
        _: &mut (),
        message: Self::Message,
        _: &(),
    ) -> omega::surface::Task<Self::Message> {
        match message {}
    }
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> Ui {
        if self.mode.get().focus {
            Text::new("focus").into()
        } else {
            Text::new("open").into()
        }
    }
}

#[test]
fn a_topics_address_comes_from_where_it_is_defined() {
    // Record addresses derive from the defining crate and type.
    assert_eq!(Mode::PLUGIN, env!("CARGO_PKG_NAME"));
    assert_eq!(Mode::KEY, "mode");
    assert_eq!(Mode::address(), format!("plugin.{}.mode", Mode::PLUGIN));
}

#[test]
fn owning_state_declares_the_right_to_publish_it() {
    let manifest = manifest_of(
        &omega::Plugin::named("desk", "0.1.0")
            .surface_default::<Showing>()
            .command::<Focus>(),
    );

    // Declare write capability and record subscriptions.
    assert_eq!(
        manifest.granted().unwrap(),
        vec![Capability::StateRead, Capability::StateWrite]
    );
    assert_eq!(manifest.state_topics, vec![Mode::address()]);
}

#[test]
fn a_widget_reads_state_that_has_never_been_set() {
    // Nobody has published a mode, so the type's own default is what a
    // reader sees — there is no half-built state to guard against.
    assert_eq!(Drawn::of::<Showing>(&State::new()).unwrap().text(), "open");
}

#[tokio::test]
async fn publishing_state_is_an_effect_like_any_other() {
    let called = Called::raw::<Focus>(&State::new(), Vec::new()).await;

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
            &<omega::config::Values as omega::config::FromValue>::from_value(
                published.value.as_ref().unwrap()
            )
            .unwrap()
        ),
        Mode { focus: true }
    );
}

#[tokio::test]
async fn one_plugin_draws_what_another_published() {
    let mut daemon = TestDaemon::serving(
        omega::Plugin::named("desk", "0.1.0")
            .surface_default::<Showing>()
            .command::<Focus>(),
    );
    daemon.welcome(&State::new()).await;
    assert_eq!(
        daemon
            .render("desk", "test", Default::default())
            .await
            .text(),
        "open"
    );

    // The daemon replicates a keyspace like any other topic, so a widget
    // watching one re-renders when it moves — whoever moved it.
    daemon
        .publish(&State::new().keyspace(&Mode::address(), Mode { focus: true }.write()))
        .await;
    assert_eq!(daemon.next_view().await.view.text(), "focus");
}

// ---- the contract the shell reads ----

/// Assert the SDK wire shapes consumed by the QML renderer.
fn wire(ui: Ui) -> serde_json::Value {
    serde_json::to_value(ui.into_tree()).unwrap()
}

/// Shared exhaustive node fixture for property-shape and vocabulary checks.
#[derive(omega::Form)]
struct FormValues {
    #[omega(label = "Name")]
    name: String,
}
macro_rules! ui_command {
    ($ty:ident, $input:ty, $name:literal) => {
        #[derive(omega::Command)]
        #[omega(name = $name)]
        struct $ty {}
        impl Command for $ty {
            type Input = $input;
            type Output = ();
            async fn call(&self, _: $input) -> omega::Result<()> {
                Ok(())
            }
        }
    };
}
ui_command!(UiToggle, (), "toggle");
ui_command!(UiConnect, String, "connect");
ui_command!(UiVolume, Percent, "set");
ui_command!(UiMute, bool, "mute");
ui_command!(UiSelect, String, "select");
ui_command!(UiForget, (), "forget");
ui_command!(UiCancel, (), "cancel");
ui_command!(UiBand, String, "band");
ui_command!(UiSave, FormValues, "save");

fn every_node() -> Ui {
    Row::new()
        .gap(6)
        .child(Text::new("80%").bold().warning())
        .child(Icon::new(Glyph::Battery))
        .child(Progress::new(Percent::of(0.7)))
        .child(
            Button::new("toggle")
                .icon(Glyph::Play)
                .flat()
                .on_press(UiToggle),
        )
        .child(Button::new("Connect").on_press(UiConnect.with("home".to_string())))
        .child(Slider::new(Percent::whole(60)).on_change(UiVolume))
        .child(Toggle::new(true).on_change(UiMute))
        .child(
            Field::new("Passphrase")
                .size(omega::ui::Size::Title)
                .secret()
                .on_submit(UiConnect),
        )
        .child(
            List::new()
                .child(Text::new("home").key("home"))
                .on_activate(UiSelect),
        )
        .child(Header::new("Networks"))
        .child(Separator::new())
        .child(Spacer::new().width(8))
        .child(Spacer::new())
        .child(Button::new("Forget").on_press(UiForget).disabled())
        .child(Button::new("Connecting").on_press(UiCancel).busy())
        .child(Graph::new(vec![14.0, 19.0, 12.0]).range(0.0, 100.0))
        .child(
            Choice::new()
                .option("auto".to_string(), Text::new("Auto"))
                .option("5".to_string(), Text::new("5 GHz"))
                .selected(Some("auto".to_string()))
                .on_select(UiBand),
        )
        .child(Grid::new(2).gap(4).child(Text::new("Sent")))
        .child(Image::new("/tmp/art.png"))
        .child(Image::new("https://example.invalid/art.png"))
        .child(omega::ui::Form::new(UiSave))
        .into()
}

#[test]
fn every_node_kind_carries_the_props_the_renderer_reads() {
    let tree = wire(every_node());

    let root = &tree["root"];
    assert_eq!(root["type"], "stack");
    assert_eq!(root["props"]["align"]["stringValue"], "row");
    // Protobuf int64 JSON values require string-to-number conversion.
    assert_eq!(root["props"]["gap"]["intValue"], "6");

    let children = root["children"].as_array().unwrap();
    assert_eq!(children[0]["type"], "text");
    assert_eq!(children[0]["props"]["text"]["stringValue"], "80%");
    assert_eq!(children[0]["props"]["bold"]["boolValue"], true);
    assert_eq!(children[0]["props"]["tone"]["stringValue"], "warning");

    assert_eq!(children[1]["type"], "icon");
    assert_eq!(children[1]["props"]["name"]["stringValue"], "battery");

    assert_eq!(children[2]["type"], "progress");
    assert_eq!(children[2]["props"]["value"]["doubleValue"], 0.7);

    assert_eq!(children[3]["type"], "button");
    assert_eq!(children[3]["props"]["icon"]["stringValue"], "play");
    assert_eq!(children[3]["props"]["label"]["stringValue"], "toggle");
    // Bindings are separate from node properties.
    assert_eq!(children[3]["events"]["press"]["command"], "toggle");

    // Bound arguments use protobuf JSON Value encoding.
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

    assert!(children[5]["events"]["change"].get("args").is_none());

    // A field says how it draws, not what it holds: the buffer is the
    // shell's until the user commits it.
    assert_eq!(children[7]["type"], "field");
    assert_eq!(children[7]["props"]["size"]["stringValue"], "title");
    assert_eq!(children[7]["props"]["label"]["stringValue"], "Passphrase");
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

    // Both carry the flag; the width is what decides between them, which is
    // the same way round the shell reads the pair.
    assert_eq!(children[11]["type"], "spacer");
    assert_eq!(children[11]["props"]["width"]["intValue"], "8");
    assert_eq!(children[11]["props"]["fill"]["boolValue"], true);
    assert_eq!(children[12]["type"], "spacer");
    assert!(children[12]["props"].get("width").is_none());
    assert_eq!(children[12]["props"]["fill"]["boolValue"], true);

    // Two reasons a control cannot be used, and the shell is told which:
    // one is waiting on an answer and the other is simply not available.
    assert_eq!(children[13]["props"]["disabled"]["boolValue"], true);
    assert_eq!(children[14]["props"]["busy"]["boolValue"], true);

    // The one prop that is not a single value. `Value` has carried a list all
    // along; nothing needed one until something had to draw a series.
    let graph = &children[15];
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

    assert_eq!(children[16]["type"], "group");
    assert_eq!(children[16]["props"]["selected"]["stringValue"], "auto");
    assert_eq!(children[16]["children"][1]["key"], "5");

    assert_eq!(children[17]["type"], "grid");
    assert_eq!(children[17]["props"]["columns"]["intValue"], "2");

    // Reject remote image URLs without fetching them.
    assert_eq!(
        children[18]["props"]["source"]["stringValue"],
        "/tmp/art.png"
    );
    assert!(children[19]["props"].get("source").is_none());

    // Stable keys preserve renderer node identity.
    assert_eq!(root["key"], "root");
    assert_eq!(children[3]["key"], "root.3");
}

#[test]
fn the_sdk_emits_only_props_the_vocabulary_declares() {
    // Every emitted property must exist in the shared node vocabulary.
    let tree = wire(every_node());
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();

    walk(&tree["root"], &mut |node| {
        let kind = node["type"].as_str().expect("a node names its kind");
        let kind = omega_proto::NodeKind::from_name(kind)
            .unwrap_or_else(|| panic!("the SDK emits the kind {kind:?}, which the table lacks"));
        seen.insert(kind.name());

        let Some(props) = node["props"].as_object() else {
            return;
        };
        for name in props.keys() {
            assert!(
                kind.every_prop().any(|prop| prop.name == name),
                "the SDK sets {name:?} on a {kind}, which the table does not declare — \
                 nothing generates a reader for it, so no shell can draw it"
            );
        }
    });

    // Require coverage of every supported node kind.
    for kind in omega_proto::NodeKind::ALL {
        assert!(
            seen.contains(kind.name()),
            "{kind} is in the vocabulary and this tree never builds one"
        );
    }
}

/// Every node in a tree, parents before children.
fn walk(node: &serde_json::Value, each: &mut impl FnMut(&serde_json::Value)) {
    if !node.is_object() {
        return;
    }
    each(node);
    if let Some(children) = node["children"].as_array() {
        for child in children {
            walk(child, each);
        }
    }
}

#[test]
fn a_widget_that_draws_nothing_says_so() {
    // Empty views contribute no root or layout gap.
    assert_eq!(wire(Ui::empty())["root"], serde_json::Value::Null);
}

#[tokio::test]
async fn independent_instances_keep_their_own_addresses() {
    let mut daemon =
        TestDaemon::serving(omega::Plugin::named("charge", "0.1.0").surface_default::<Charge>());
    daemon.welcome(&State::new().battery(0.5, false)).await;
    daemon
        .render("charge", "window-1", Default::default())
        .await;
    daemon
        .render("charge", "top-bar-1", Default::default())
        .await;
    daemon.publish(&State::new().battery(0.2, false)).await;
    let anonymous = daemon.next_view().await;
    let placed = daemon.next_view().await;
    assert_eq!(anonymous.instance.id.as_str(), "test-charge-window-1");
    assert_eq!(placed.instance.id.as_str(), "test-charge-top-bar-1");
    assert_eq!(anonymous.view.text(), "20%");
    assert_eq!(placed.view.text(), "20%");
}

// ---- publishing a collection ----

/// One access point, as a plugin that scanned for them would report it.
#[derive(omega::Config, Debug, Clone, Default, PartialEq)]
struct AccessPoint {
    ssid: String,
    signal: u32,
    secured: bool,
}

/// A plugin's own state, which is a list — the shape most plugins actually have
/// to publish, and the one that did not compile until `Vec<T>` was a value.
#[derive(omega::PluginState, Debug, Clone, Default, PartialEq)]
struct Scan {
    found: Vec<AccessPoint>,
    names: Vec<String>,
}

#[test]
fn a_plugin_can_publish_a_list() {
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

    // Round-trip nested structs through generic values.
    let round_tripped = Scan::read(&scan.write());
    assert_eq!(round_tripped, scan);
}

#[test]
fn a_list_that_cannot_be_read_takes_the_default() {
    // Invalid field values use the derived reader's default.
    let wrong = Values::new().with("names", 7_i64);
    assert_eq!(Scan::read(&wrong).names, Vec::<String>::new());
}

#[test]
fn one_unreadable_element_is_not_a_shorter_list() {
    // Reject the complete list if any element cannot be decoded.
    let mixed: Vec<omega::internal::Value> = vec![
        omega::internal::IntoValue::into_value("home"),
        omega::internal::IntoValue::into_value(7_i64),
    ];
    let read: Option<Vec<String>> =
        omega::internal::FromValue::from_value(&omega::internal::IntoValue::into_value(mixed));
    assert!(read.is_none());
}

// ---- the clock ----

#[derive(omega::Surface)]
struct BarClock {
    clock: Clock,
}

impl Surface for BarClock {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();
    fn update(
        &self,
        _: &mut (),
        message: Self::Message,
        _: &(),
    ) -> omega::surface::Task<Self::Message> {
        match message {}
    }
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> Ui {
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
    // Clock readings already contain timezone-adjusted parts.
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

    assert_eq!(Drawn::of::<BarClock>(&state).unwrap().text(), "Tue 14:32");
}

#[test]
fn a_reported_absent_clock_formats_default_values() {
    // Accessors return defaults for missing clock readings.
    assert_eq!(
        Drawn::of::<BarClock>(&State::new().absent(omega::testing::SystemTopic::Time))
            .unwrap()
            .text(),
        "Sun 00:00"
    );
}

// Raw topic access

/// Reading a topic that has no typed accessors, only the floor `get()` gives.
#[derive(omega::Surface)]
struct Devices {
    bluetooth: omega::platform::bluetooth::Bluetooth,
}

impl Surface for Devices {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();
    fn update(
        &self,
        _: &mut (),
        message: Self::Message,
        _: &(),
    ) -> omega::surface::Task<Self::Message> {
        match message {}
    }
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> Ui {
        if !self.bluetooth.has_reading() {
            return Ui::empty();
        }
        let charged = self
            .bluetooth
            .connected_devices()
            .into_iter()
            .filter_map(|device| device.battery())
            .count();
        Text::new(format!(
            "{} paired, {charged} reporting",
            self.bluetooth.known_devices().len()
        ))
        .into()
    }
}

#[test]
fn a_handle_reads_its_topic_through_the_types_it_carries() {
    let state = State::new().with(omega_proto::omega::BluetoothState {
        available: true,
        powered: true,
        discovering: false,
        devices: vec![omega_proto::omega::BluetoothDevice {
            id: "/org/bluez/hci0/dev_60_AB_D2_25_8C_49".into(),
            can_connect: true,
            address: "60:AB:D2:25:8C:49".into(),
            name: "Bose NC 700".into(),
            connected: true,
            paired: true,
            icon: "audio-headphones".into(),
            battery_percent: Some(72),
        }],
    });

    assert_eq!(
        Drawn::of::<Devices>(&state).unwrap().text(),
        "1 paired, 1 reporting"
    );
    assert!(Drawn::of::<Devices>(&State::new()).unwrap().is_empty());
}

#[test]
fn a_handle_declares_the_topic_its_type_names() {
    // The manifest comes from the fields, so holding `Bluetooth` is what asks
    // for the topic — there is no string anywhere to get wrong.
    let manifest = omega::testing::manifest_of(
        &omega::Plugin::named("devices", "0.1.0").surface_default::<Devices>(),
    );
    assert_eq!(manifest.state_topics, vec!["bluetooth"]);
}

// ---- composites ----------------------------------------------------------

/// Composite reading fixture.
#[derive(omega::Surface)]
struct Situation {
    power: omega::platform::power::Power,
}

impl Surface for Situation {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();
    fn update(
        &self,
        _: &mut (),
        message: Self::Message,
        _: &(),
    ) -> omega::surface::Task<Self::Message> {
        match message {}
    }
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> Ui {
        Text::new(self.power.status().label()).into()
    }
}

#[test]
fn a_composite_declares_every_topic_it_is_made_of() {
    // Composite dependencies contribute all their topics to the manifest.
    let manifest =
        manifest_of(&omega::Plugin::named("situation", "0.1.0").surface_default::<Situation>());
    assert_eq!(manifest.state_topics, vec!["battery", "mains"]);
}

#[test]
fn a_full_battery_on_the_wall_is_neither_charging_nor_on_battery() {
    // The case every hand-written version of this got wrong.
    let full = State::new().battery(1.0, false).mains(true);
    assert_eq!(
        Drawn::of::<Situation>(&full).unwrap().text(),
        "Fully charged"
    );

    let held = State::new().battery(0.8, false).mains(true);
    assert_eq!(Drawn::of::<Situation>(&held).unwrap().text(), "On mains");

    let charging = State::new().battery(0.8, true).mains(true);
    assert_eq!(
        Drawn::of::<Situation>(&charging).unwrap().text(),
        "Charging"
    );

    let draining = State::new().battery(0.8, false).mains(false);
    assert_eq!(
        Drawn::of::<Situation>(&draining).unwrap().text(),
        "On battery"
    );
}

#[test]
fn a_machine_with_no_battery_is_on_mains_not_flat() {
    // Missing battery charge must remain None.
    let desktop = State::new().absent(SystemTopic::Battery).mains(true);
    assert_eq!(Drawn::of::<Situation>(&desktop).unwrap().text(), "On mains");
}

#[derive(omega::PluginState, Default, Clone)]
struct Counter {
    value: u32,
}

#[derive(omega::Command)]
struct IncrementTwice {
    counter: omega::record::Own<Counter>,
}

impl Command for IncrementTwice {
    type Input = Args;
    type Output = String;
    async fn call(&self, _args: Args) -> Result<Self::Output, omega::Error> {
        self.counter
            .update(|counter| counter.value += 1)
            .receipt()
            .unwrap()
            .detach();
        self.counter
            .update(|counter| counter.value += 1)
            .receipt()
            .unwrap()
            .detach();
        Ok(self.counter.get().value.to_string())
    }
}

#[tokio::test]
async fn consecutive_record_updates_see_local_writes_before_replication() {
    let called = Called::raw::<IncrementTwice>(&State::new(), Vec::new()).await;
    assert_eq!(answered(&called.answer).as_deref(), Some("2"));
    assert_eq!(called.effects.len(), 2);
    for (index, effect) in called.effects.iter().enumerate() {
        let omega_proto::omega::invoke::Op::SetState(write) = effect else {
            panic!("expected a record write")
        };
        let values: Values =
            omega_proto::FromValue::from_value(write.value.as_ref().unwrap()).unwrap();
        assert_eq!(Counter::read(&values).value, index as u32 + 1);
    }
}

#[derive(omega::Command)]
struct FillRecords {
    counter: omega::record::Own<Counter>,
}
impl Command for FillRecords {
    type Input = Args;
    type Output = u32;
    async fn call(&self, _: Args) -> Result<Self::Output, omega::Error> {
        let mut admitted = 0;
        loop {
            match self.counter.update(|counter| counter.value += 1).receipt() {
                Ok(receipt) => {
                    admitted += 1;
                    receipt.detach();
                }
                Err(omega::effect::EffectError::Full) => break,
                Err(error) => panic!("unexpected admission failure: {error}"),
            }
        }
        assert_eq!(self.counter.get().value, admitted);
        assert!(matches!(
            self.counter
                .update(|_| panic!("rejected update ran"))
                .receipt(),
            Err(omega::effect::EffectError::Full)
        ));
        Ok(admitted)
    }
}

#[tokio::test]
async fn rejected_record_admission_does_not_change_local_state_or_run_the_update() {
    let called = Called::raw::<FillRecords>(&State::new(), vec![]).await;
    assert_eq!(called.effects.len(), 64);
}

#[derive(omega::Command)]
struct ForwardRecord {
    counter: omega::record::Own<Counter>,
}
impl Command for ForwardRecord {
    type Input = Args;
    type Output = ();
    async fn call(&self, _: Args) -> Result<Self::Output, omega::Error> {
        self.counter.update(|counter| counter.value += 1).await
    }
}
#[tokio::test]
async fn command_fixtures_complete_forwarded_record_publications() {
    let called = Called::raw::<ForwardRecord>(&State::new(), vec![]).await;
    assert_eq!(called.effects.len(), 1);
    assert!(called.answer.is_ok());
}

#[derive(omega::PluginState, Default)]
struct LargeRecord {
    text: String,
}

#[derive(omega::Command)]
struct FillRecordBytes {
    record: Own<LargeRecord>,
}
impl Command for FillRecordBytes {
    type Input = Args;
    type Output = bool;
    async fn call(&self, _: Args) -> Result<Self::Output, omega::Error> {
        let value = LargeRecord {
            text: "x".repeat(3 * 1024 * 1024),
        };
        self.record.set(&value).receipt().unwrap().detach();
        self.record.set(&value).receipt().unwrap().detach();
        assert!(matches!(
            self.record
                .update(|_| panic!("byte reservation failure ran the callback"))
                .receipt(),
            Err(omega::effect::EffectError::Full)
        ));
        assert_eq!(self.record.get().text, value.text);
        Ok(true)
    }
}

#[tokio::test]
async fn exhausted_record_byte_budget_preserves_the_local_record() {
    let called = Called::raw::<FillRecordBytes>(&State::new(), vec![]).await;
    assert_eq!(called.effects.len(), 2);
}

#[derive(omega::Command)]
struct OversizedRecord {
    record: Own<LargeRecord>,
}
impl Command for OversizedRecord {
    type Input = Args;
    type Output = bool;
    async fn call(&self, _: Args) -> Result<Self::Output, omega::Error> {
        assert!(matches!(
            self.record
                .update(|record| record.text = "x".repeat(omega_proto::MAX_FRAME_LEN))
                .receipt(),
            Err(omega::effect::EffectError::TooLarge)
        ));
        assert!(self.record.get().text.is_empty());
        self.record
            .update(|record| record.text = "valid".into())
            .receipt()
            .unwrap()
            .detach();
        Ok(true)
    }
}

#[tokio::test]
async fn oversized_record_result_is_not_committed_and_releases_its_reservation() {
    let called = Called::raw::<OversizedRecord>(&State::new(), vec![]).await;
    assert_eq!(called.effects.len(), 1);
}

#[test]
fn missing_mains_is_not_evidence_of_battery_operation() {
    use omega::testing::SystemTopic;
    let state = State::new().battery(0.5, false).absent(SystemTopic::Mains);
    assert_eq!(Drawn::of::<Situation>(&state).unwrap().text(), "Unknown");
}
