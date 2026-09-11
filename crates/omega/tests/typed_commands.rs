use omega::config::{IntoValue, Values};
use omega::testing::{Called, Drawn, State};
use omega::ui::{Button, Form, Metric, Section, Slider};
use omega::{Args, Command, Input, Percent, Plugin, Ui, Widget};

#[derive(omega::Command)]
#[omega(name = "volume")]
struct SetVolume {
    volume: omega::effect::Volume,
}
impl Command for SetVolume {
    type Input = Percent;
    type Output = ();
    async fn call(&self, value: Percent) -> omega::Result<()> {
        self.volume.set(value).await
    }
}

#[derive(omega::Widget)]
struct Controls {}
impl Widget for Controls {
    fn render(&self) -> Ui {
        Section::new("Audio")
            .child(Metric::new(Percent::whole(40)).label("Output volume"))
            .child(
                Slider::new(Percent::whole(40))
                    .on_change(SetVolume)
                    .key("volume"),
            )
            .child(
                Button::new("Quiet")
                    .on_press(SetVolume.with(Percent::whole(10)))
                    .key("quiet"),
            )
            .into()
    }
}

#[test]
fn bindings_share_registration_identity_without_acquiring_command_capabilities() {
    let plugin = Plugin::named("audio", "1")
        .widget::<Controls>()
        .command::<SetVolume>();
    let manifest = plugin.manifest().unwrap();
    assert!(
        manifest
            .surfaces
            .iter()
            .any(|surface| surface.id == "volume")
    );
    assert!(<Controls as omega::Wired>::capabilities().is_empty());
    let drawn = Drawn::of::<Controls>(&State::new());
    let binding = &drawn.node("volume").unwrap().events["change"];
    assert_eq!(binding.command, "volume");
    assert!(binding.args.is_empty());
    let quiet = &drawn.node("quiet").unwrap().events["press"];
    assert_eq!(quiet.command, "volume");
    assert_eq!(
        Args::new(quiet.args.clone()).get::<Percent>(0),
        Some(Percent::whole(10))
    );
}

#[tokio::test]
async fn malformed_input_is_refused_without_effects() {
    for args in [
        vec![],
        vec![true.into_value()],
        vec![1.1.into_value()],
        vec![0.5.into_value(), 0.2.into_value()],
    ] {
        let called = Called::raw::<SetVolume>(&State::new(), args).await;
        assert!(called.answer.is_err());
        assert!(called.effects.is_empty());
    }
    let valid = Called::of::<SetVolume>(&State::new(), Percent::whole(40)).await;
    assert!(valid.answer.is_ok());
    assert_eq!(valid.effects.len(), 1);
}

#[derive(omega::Form, Debug, PartialEq)]
struct Credentials {
    #[omega(label = "Network", placeholder = "Home")]
    ssid: String,
    #[omega(label = "Password", help = "Leave blank for saved networks", secret)]
    password: String,
}
#[derive(omega::Command)]
struct Connect {}
impl Command for Connect {
    type Input = Credentials;
    type Output = ();
    async fn call(&self, _: Credentials) -> omega::Result<()> {
        Ok(())
    }
}

#[test]
fn form_definition_owns_labels_keys_and_strict_decoding() {
    let drawn = Drawn::of_ui(Form::new(Connect).submit_label("Join").into());
    let root = drawn.node("root").unwrap();
    assert_eq!(root.events["submit"].command, "connect");
    let first = &root.children[0];
    assert_eq!(drawn.prop(&first.key, "name").as_deref(), Some("ssid"));
    assert_eq!(drawn.prop(&first.key, "label").as_deref(), Some("Network"));
    assert_eq!(
        drawn.prop(&first.key, "placeholder").as_deref(),
        Some("Home")
    );
    let password = &root.children[1];
    assert_eq!(drawn.flag(&password.key, "secret"), Some(true));
    assert_eq!(
        drawn.prop(&password.key, "help").as_deref(),
        Some("Leave blank for saved networks")
    );

    let valid = Credentials {
        ssid: "Home".into(),
        password: String::new(),
    };
    assert_eq!(
        Credentials::decode(Args::new(valid.encode())).unwrap().ssid,
        "Home"
    );
    for fields in [
        Values::new().with("ssid", "Home"),
        Values::new().with("ssid", "Home").with("password", true),
        Values::new()
            .with("ssid", "Home")
            .with("password", "secret")
            .with("extra", "value"),
    ] {
        let error = Credentials::decode(Args::new(vec![fields.into_value()])).unwrap_err();
        assert!(!error.to_string().contains("secret"));
    }
}

#[derive(omega::Input, Debug, PartialEq)]
struct Level {
    value: Percent,
    muted: bool,
}
#[test]
fn structured_inputs_preserve_field_types() {
    let level = Level {
        value: Percent::whole(30),
        muted: false,
    };
    assert_eq!(
        Level::decode(Args::new(level.encode())).unwrap().value,
        Percent::whole(30)
    );
}

#[derive(omega::Command)]
struct Ping;
impl Command for Ping {
    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        Ok(())
    }
}
#[test]
fn unit_commands_bind_and_duplicate_names_are_refused() {
    let drawn = Drawn::of_ui(Button::new("Ping").on_press(Ping).into());
    assert_eq!(drawn.node("root").unwrap().events["press"].command, "ping");
    assert!(
        Plugin::named("test", "1")
            .command::<Ping>()
            .command::<Ping>()
            .manifest()
            .is_err()
    );
}

#[derive(omega::Command)]
struct Submit;
impl Command for Submit {
    type Input = Credentials;
    type Output = ();
    async fn call(&self, _: Credentials) -> omega::Result<()> {
        Ok(())
    }
}
#[test]
fn unit_commands_support_forms_and_bound_inputs() {
    let form = Drawn::of_ui(Form::new(Submit).into());
    assert_eq!(
        form.node("root").unwrap().events["submit"].command,
        "submit"
    );
    let button = Button::new("Submit").on_press(Submit.with(Credentials {
        ssid: "Home".into(),
        password: String::new(),
    }));
    assert_eq!(
        Drawn::of_ui(button.into()).node("root").unwrap().events["press"]
            .args
            .len(),
        1
    );
}

#[tokio::test]
async fn input_refusals_travel_through_the_real_runtime() {
    let mut daemon =
        omega::testing::TestDaemon::serving(Plugin::named("audio", "1").command::<SetVolume>());
    daemon.welcome(&State::new()).await;
    assert_eq!(
        daemon.call("volume", vec![true.into_value()]).await,
        Err("expected Percent".to_string())
    );
    assert!(
        daemon
            .call("volume", Percent::whole(40).encode())
            .await
            .is_ok()
    );
}

#[test]
fn repeated_components_keep_unique_keys_and_replace_labels() {
    let first = Metric::new("10").label("Old").label("First").primary();
    let second = Metric::new("20").label("Second").warning();
    let drawn = Drawn::of_ui(Section::new("Readings").child(first).child(second).into());
    let keys = drawn.keys();
    assert_eq!(
        keys.iter().collect::<std::collections::BTreeSet<_>>().len(),
        keys.len()
    );
    assert_eq!(drawn.text(), "Readings 10 First 20 Second");
}
