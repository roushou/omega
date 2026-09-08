//! A Wi-Fi indicator and its panel — the test of whether the SDK is worth it.
//!
//! `docs/design.md` names this: Omarchy's `omarchy.network` is 1,970 lines of
//! QML and 380 of JavaScript, and if the same desktop feature does not come
//! out here at a small fraction of that, the typed path is not shorter than
//! the untyped one and the SDK is decoration.
//!
//! It is an example rather than a test because what it proves is that this
//! compiles and reads well, which a person judges. `cargo build --examples`
//! keeps it honest: the SDK cannot change out from under it silently.
//!
//! **What it is not.** There is no list of networks to pick from, because
//! Omega cannot express one. Two gaps, and neither is a missing feature so
//! much as a hole the ontology has:
//!
//!  - `NetworkState` describes the connection the machine *has*. There is
//!    nothing in it for the ones it could have, so a scan result has nowhere
//!    to live. That wants a topic of its own rather than another field:
//!    forty access points whose signal jitters would bump the `network`
//!    revision and wake every indicator that only ever wanted the SSID.
//!  - A unit cannot work around that in its own keyspace either. `Value` has
//!    `ListValue` and `MapValue`, but `IntoValue`/`FromValue` are implemented
//!    for scalars only — so a unit cannot hold a list of anything, scanned or
//!    otherwise.
//!
//! Both are recorded in `docs/design.md`. Until they are closed, joining a
//! network is typing its name, which is what the two fields below are.

use omega::{
    Answer, Args, Bind, Button, Column, Command, Field, Icon, Network, Percent, Progress, Row,
    Shell, Stack, Text, Ui, Widget,
};

/// This unit's name, for the config plane to refer to it by.
pub const UNIT: &str = "wifi";

/// What the document can configure an instance with.
#[derive(omega::Config, Debug, Clone, PartialEq)]
pub struct Settings {
    /// Below this, the signal is drawn as urgent.
    pub weak: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self { weak: 30 }
    }
}

/// The bar slot: what is on, and how well.
#[derive(omega::Widget, Debug)]
pub struct Indicator {
    network: Network,
    #[omega(config)]
    settings: Settings,
}

impl Widget for Indicator {
    fn render(&self) -> Ui {
        if !self.network.has_reading() {
            return Ui::empty();
        }

        let Some(ssid) = self.network.ssid() else {
            // Wired, or nothing at all. Both are a plug or a slash, and
            // neither has a name worth drawing.
            return match self.network.is_connected() {
                true => Icon::new("link").into(),
                false => Icon::new("globe").dim().into(),
            };
        };

        let strength = self.network.strength();
        Row::new()
            .gap(6)
            .child(Icon::new(bars(strength)))
            .child(match strength < self.settings.weak {
                true => Text::new(ssid).color("urgent"),
                false => Text::new(ssid),
            })
            .into()
    }
}

/// The popout: the details, and the two things anyone does with them.
#[derive(omega::Widget, Debug)]
pub struct Panel {
    network: Network,
}

impl Widget for Panel {
    fn render(&self) -> Ui {
        if !self.network.is_connected() {
            return Column::new()
                .gap(8)
                .child(Text::new("Not connected").dim())
                .child(join())
                .into();
        }

        let name = self.network.ssid().unwrap_or_else(|| "Wired".to_string());
        Column::new()
            .gap(8)
            .child(Text::new(&name).bold())
            .child(
                Row::new()
                    .gap(6)
                    .child(Icon::new("wifi").dim())
                    .child(Progress::new(self.network.strength()))
                    .child(Text::new(self.network.strength()).dim()),
            )
            .child(Button::new("Disconnect").on_press(Bind::call("disconnect").arg(name)))
            .child(join())
            .into()
    }
}

/// Somewhere to type a network and its passphrase.
///
/// Two fields, one binding: the SSID is an argument the view chose and the
/// passphrase is what the user typed, appended by the shell — so `connect` is
/// called with both and neither is a command of its own.
fn join() -> Stack {
    Column::new()
        .gap(4)
        .child(Text::new("Join another").dim())
        .child(Field::new("Network").on_submit("remember"))
        .child(
            Field::new("Passphrase")
                .secret()
                .on_submit(Bind::call("connect")),
        )
}

/// Join a network. Called with the passphrase the field carried.
#[derive(omega::Command, Debug)]
pub struct Connect {
    shell: Shell,
}

impl Command for Connect {
    fn call(&self, args: Args) -> Answer {
        let Some(secret) = args.get::<String>(0) else {
            return Answer::refused("no passphrase");
        };
        // The escape hatch, and honestly so: connecting is NetworkManager's
        // and Omega has no action for it yet.
        self.shell
            .run(format!("nmcli device wifi connect --ask password {secret}"));
        Answer::value("connecting")
    }
}

/// Leave the network named in the binding.
#[derive(omega::Command, Debug)]
pub struct Disconnect {
    shell: Shell,
}

impl Command for Disconnect {
    fn call(&self, args: Args) -> Answer {
        let Some(name) = args.get::<String>(0) else {
            return Answer::refused("no network");
        };
        self.shell.run(format!("nmcli connection down id {name}"));
        Answer::value("disconnecting")
    }
}

/// Which icon a strength reads as.
///
/// One glyph today: the shell's icon set has `wifi` and nothing weaker, so
/// strength is carried by the colour instead. A richer set would branch here
/// and nothing else would change.
fn bars(_strength: Percent) -> &'static str {
    "wifi"
}

fn main() -> omega::Result<()> {
    omega::Plugin::named(UNIT, "0.1.0")
        .widget_as::<Indicator>("indicator")
        .widget_as::<Panel>("panel")
        .command::<Connect>("connect")
        .command::<Disconnect>("disconnect")
        .run()
}
