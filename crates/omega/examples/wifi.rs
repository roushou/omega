//! Wi-Fi readings and connection controls. Forms submit once; passwords stay out
//! of records. Requests acknowledge activation, and Wifi reports its outcome.

use omega::platform::network::{AccessPoint, Network, Wifi, WifiControl, WifiPhase};
use omega::record::{Own, Watch};
use omega::ui::{
    Button, Form, Glyph, Graph, Icon, List, Progress, Row, Section, Size, Stack, Text,
};
use omega::{Command, Percent, Surface, Ui};

/// This plugin's name, for the config plane to refer to it by.
pub const PLUGIN: &str = env!("CARGO_PKG_NAME");

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
#[derive(omega::Surface, Debug)]
pub struct Indicator {
    network: Network,
    #[omega(config)]
    settings: Settings,
}

impl Surface for Indicator {
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

/// Signal history stored in the plugin record. Retained while the daemon runs.
#[derive(omega::PluginState, Debug, Clone, Default, PartialEq)]
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
#[derive(omega::Surface, Debug)]
pub struct Panel {
    network: Network,
    wifi: Wifi,
    signal: Watch<Signal>,
}

impl Surface for Panel {
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
                // Use the full signal range to keep small fluctuations in proportion.
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

/// Available Wi-Fi networks. Row keys are SSIDs passed to the connection command.
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
    const ID: &'static str = "connect";

    type Input = Credentials;
    type Output = ();
    async fn call(&self, input: Credentials) -> omega::Result<()> {
        self.wifi.connect(input.ssid, input.password).await
    }
}

/// Append a signal sample to history. Schedule this command from the system
/// document at the desired sampling interval.
///
/// ```ignore
/// Schedules::every("sample-wifi", Cadence::seconds(30), Actions::invoke(Sample))
/// ```
#[derive(omega::Command, Debug)]
pub struct Sample {
    network: Network,
    signal: Own<Signal>,
}

impl Command for Sample {
    const ID: &'static str = "sample";

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
    const ID: &'static str = "join";

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
    const ID: &'static str = "disconnect";

    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        self.wifi.disconnect().await
    }
}

/// Select the Wi-Fi glyph. Signal strength is represented by color.
fn bars(_strength: Percent) -> Glyph {
    Glyph::Wifi
}

pub fn plugin() -> omega::Plugin {
    omega::Plugin::new(PLUGIN, env!("CARGO_PKG_VERSION"))
        .surface_as::<Indicator>("indicator")
        .surface_as::<Panel>("panel")
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
