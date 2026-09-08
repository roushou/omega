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
//! The indicator holds `Network` — the one connection the machine has — and
//! the panel holds `Wifi`, the list it could have. Holding only what it draws
//! is what keeps the indicator from waking every time a signal jitters three
//! rooms away.

use omega::{
    AccessPoint, Answer, Args, Bind, Button, Column, Command, Field, Graph, Icon, List, Network,
    Own, Percent, Progress, Row, Shell, Stack, Text, Ui, Watch, Widget, Wifi,
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

/// How the signal has been, kept by this unit because nobody keeps it for it.
///
/// The daemon publishes readings, not histories — a topic that carried one
/// would be a new revision on every sample. So a series is something a unit
/// accumulates in its own keyspace, which outlives the process that wrote it.
#[derive(omega::Topic, Debug, Clone, Default, PartialEq)]
pub struct Signal {
    pub recent: Vec<f64>,
}

impl Signal {
    /// How much history a bar-sized graph can show. Beyond this the points
    /// are narrower than a pixel and the line is a smear.
    const KEPT: usize = 40;

    fn with(&self, reading: f64) -> Self {
        let mut recent = self.recent.clone();
        recent.push(reading);
        if recent.len() > Self::KEPT {
            recent.drain(..recent.len() - Self::KEPT);
        }
        Self { recent }
    }
}

/// The popout: the details, and the two things anyone does with them.
#[derive(omega::Widget, Debug)]
pub struct Panel {
    network: Network,
    wifi: Wifi,
    signal: Watch<Signal>,
}

impl Widget for Panel {
    fn render(&self) -> Ui {
        let mut panel = Column::new().gap(8);

        if self.network.is_connected() {
            let name = self.network.ssid().unwrap_or_else(|| "Wired".to_string());
            panel = panel
                .child(Text::new(&name).bold())
                .child(
                    Row::new()
                        .gap(6)
                        .child(Icon::new("wifi").dim())
                        .child(Progress::new(self.network.strength()))
                        .child(Text::new(self.network.strength()).dim()),
                )
                // Pinned to the whole range: a signal wobbling between 70 and
                // 74 would otherwise fill the frame and read as a collapse.
                .child(Graph::new(self.signal.get().recent).range(0.0, 100.0))
                .child(Button::new("Disconnect").on_press(Bind::call("disconnect").arg(name)));
        } else {
            panel = panel.child(Text::new("Not connected").dim());
        }

        panel.child(networks(&self.wifi)).child(join()).into()
    }
}

/// Everything on the air, one row each.
///
/// The list owns the cursor: arrows move it and Enter activates, and neither
/// reaches this unit. What arrives is the key of the row that was chosen —
/// which is the SSID, because that is what the row was keyed by.
fn networks(wifi: &Wifi) -> List {
    List::new()
        .gap(2)
        .height(180)
        .children(wifi.networks().iter().map(row))
        .on_activate(Bind::call("join"))
}

fn row(point: &AccessPoint) -> Stack {
    let name = match point.is_active() {
        true => Text::new(point.ssid()).bold(),
        false => Text::new(point.ssid()),
    };
    Row::new()
        .gap(6)
        .key(point.ssid())
        .child(name)
        .child(Text::new(point.strength()).dim())
        .child(match point.is_secured() {
            true => Icon::new("lock").dim(),
            false => Icon::new("globe").dim(),
        })
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
        // The escape hatch, and honestly so: joining is NetworkManager's and
        // Omega has no action for it yet.
        self.shell
            .run(format!("nmcli device wifi connect --ask password {secret}"));
        Answer::from("connecting")
    }
}

/// Take a signal reading, for the graph to draw later.
///
/// A command rather than the widget, because writing state is doing something
/// — a widget that recorded on every render would record on every percent the
/// battery moved, too.
#[derive(omega::Command, Debug)]
pub struct Sample {
    network: Network,
    signal: Own<Signal>,
}

impl Command for Sample {
    fn call(&self, _: Args) -> Answer {
        let strength = f64::from(self.network.strength().whole_percent());
        self.signal.set(&self.signal.get().with(strength));
        Answer::done()
    }
}

/// Join the row that was chosen. Called with its key, which is the SSID.
#[derive(omega::Command, Debug)]
pub struct Join {
    shell: Shell,
}

impl Command for Join {
    fn call(&self, args: Args) -> Answer {
        let Some(ssid) = args.get::<String>(0) else {
            return Answer::refused("no network");
        };
        self.shell.run(format!("nmcli device wifi connect {ssid}"));
        Answer::from("joining")
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
        Answer::from("disconnecting")
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
        .command::<Join>("join")
        .command::<Sample>("sample")
        .command::<Disconnect>("disconnect")
        .run()
}
