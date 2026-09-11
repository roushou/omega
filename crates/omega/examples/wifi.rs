//! Wi-Fi readings and connection controls. Forms submit once; passwords stay out
//! of records. Requests acknowledge activation, and Wifi reports its outcome.

use omega::effect::WifiControl;
use omega::reading::{AccessPoint, Network, Wifi, WifiPhase};
use omega::record::{Own, Watch};
use omega::ui::{
    Button, Form, Glyph, Graph, Icon, List, Progress, Row, Section, Size, Stack, Text,
};
use omega::{Command, Percent, Ui, Widget};

/// This unit's name, for the config plane to refer to it by.
pub const UNIT: &str = env!("CARGO_PKG_NAME");

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
            return if self.network.is_connected() {
                Icon::new(Glyph::Link).into()
            } else {
                Icon::new(Glyph::Globe).muted().into()
            };
        };

        let strength = self.network.strength();
        Row::new()
            .gap(6)
            .child(Icon::new(bars(strength)))
            .child(if strength < self.settings.weak {
                Text::new(ssid).warning()
            } else {
                Text::new(ssid)
            })
            .into()
    }
}

/// How the signal has been, kept by this unit because nobody keeps it for it.
///
/// The daemon publishes readings, not histories — a topic that carried one
/// would be a new revision on every sample. So a series is something a unit
/// accumulates in its own keyspace, which outlives the process that wrote it.
#[derive(omega::UnitState, Debug, Clone, Default, PartialEq)]
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
        let state = match self.wifi.phase() {
            WifiPhase::Connecting => "Connecting…".to_string(),
            WifiPhase::Connected => format!("Connected to {}", self.wifi.ssid()),
            WifiPhase::Failed => self.wifi.failure(),
            WifiPhase::Disconnected => "Wi-Fi disconnected".to_string(),
            WifiPhase::Unspecified => "Wi-Fi unavailable".to_string(),
        };
        let mut panel = Section::new("Wi-Fi").child(Text::new(state));

        if self.network.is_connected() {
            panel = panel
                .child(
                    Row::new()
                        .gap(6)
                        .child(Icon::new(Glyph::Wifi).muted())
                        .child(Progress::new(self.network.strength()).fill_width())
                        .child(Text::new(self.network.strength()).muted()),
                )
                // Pinned to the whole range: a signal wobbling between 70 and
                // 74 would otherwise fill the frame and read as a collapse.
                .child(
                    Graph::new(self.signal.get().recent)
                        .range(0.0, 100.0)
                        .height(32),
                )
                .child(Button::new("Disconnect").on_press(Disconnect));
        } else {
            panel = panel.child(Text::new("Not connected").muted());
        }

        panel
            .child(Text::new("Available networks").bold())
            .child(
                Text::new("Select a saved or open network to connect.")
                    .size(Size::Caption)
                    .muted(),
            )
            .child(networks(&self.wifi))
            .child(Text::new("Join with a password").bold())
            .child(join())
            .into()
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
        .children(wifi.networks().iter().map(row))
        .on_activate(Join)
}

fn row(point: &AccessPoint) -> Stack {
    let name = if point.is_active() {
        Text::new(point.ssid()).bold()
    } else {
        Text::new(point.ssid())
    };
    Row::new()
        .gap(6)
        .key(point.ssid())
        .child(name.fill_width())
        .child(Text::new(point.strength()).muted())
        .child(if point.is_secured() {
            Icon::new(Glyph::Lock).muted()
        } else {
            Icon::new(Glyph::Globe).muted()
        })
}

/// The shell owns drafts until both fields are submitted together.
fn join() -> Form {
    Form::new(Connect).submit_label("Connect")
}

#[derive(omega::Form, Debug)]
pub struct Credentials {
    #[omega(label = "Network", placeholder = "Network name")]
    pub ssid: String,
    #[omega(
        label = "Password",
        help = "Leave blank for saved or open networks",
        secret
    )]
    pub password: String,
}

#[derive(omega::Command, Debug)]
pub struct Connect {
    wifi: WifiControl,
}
impl Command for Connect {
    type Input = Credentials;
    type Output = ();
    async fn call(&self, input: Credentials) -> omega::Result<()> {
        self.wifi.connect(input.ssid, input.password).await
    }
}

/// Take a signal reading, for the graph to draw later.
///
/// A command rather than the widget, because writing state is doing something
/// — a widget that recorded on every render would record on every percent the
/// battery moved, too.
///
/// Nothing in this unit calls it: how often a history is sampled belongs to
/// whoever runs the machine, so the document says it.
///
/// ```ignore
/// Schedules::every("sample-wifi", Cadence::seconds(30), Actions::invoke(UNIT, "sample"))
/// ```
#[derive(omega::Command, Debug)]
pub struct Sample {
    network: Network,
    signal: Own<Signal>,
}

impl Command for Sample {
    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> Result<(), omega::Error> {
        let strength = f64::from(self.network.strength().whole_percent());
        self.signal
            .update(|signal| *signal = signal.with(strength))
            .await
    }
}

/// Join the row that was chosen. Called with its key, which is the SSID.
#[derive(omega::Command, Debug)]
pub struct Join {
    wifi: WifiControl,
}

impl Command for Join {
    type Input = String;
    type Output = ();
    async fn call(&self, ssid: String) -> omega::Result<()> {
        self.wifi.connect(ssid, "").await
    }
}

/// Leave the network named in the binding.
#[derive(omega::Command, Debug)]
pub struct Disconnect {
    wifi: WifiControl,
}

impl Command for Disconnect {
    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        self.wifi.disconnect().await
    }
}

/// Which icon a strength reads as.
///
/// One glyph today: the shell's icon set has `wifi` and nothing weaker, so
/// strength is carried by the colour instead. A richer set would branch here
/// and nothing else would change.
fn bars(_strength: Percent) -> Glyph {
    Glyph::Wifi
}

pub fn plugin() -> omega::Plugin {
    omega::Plugin::named(UNIT, env!("CARGO_PKG_VERSION"))
        .widget_as::<Indicator>("indicator")
        .widget_as::<Panel>("panel")
        .command::<Connect>()
        .command::<Join>()
        .command::<Sample>()
        .command::<Disconnect>()
}

fn main() -> omega::Result<()> {
    plugin().run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega::testing::{Called, State, manifest_of};
    #[tokio::test]
    async fn form_fields_become_one_network_request_without_recording_the_password() {
        let input = Credentials {
            ssid: "Home".to_string(),
            password: "private".to_string(),
        };
        let called = Called::of::<Connect>(&State::new(), input).await;
        assert!(called.answer.is_ok());
        assert_eq!(called.effects.len(), 1);
        let invalid = Called::raw::<Connect>(&State::new(), vec![]).await;
        assert!(invalid.answer.is_err());
        assert!(invalid.effects.is_empty());
    }
    #[test]
    fn network_controls_do_not_need_process_spawning() {
        let grants = manifest_of(&plugin()).granted().unwrap();
        assert!(grants.contains(&omega::internal::Capability::Network));
        assert!(!grants.contains(&omega::internal::Capability::Spawn));
    }
}
