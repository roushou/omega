//! Wi-Fi connection requests, authorized independently of process spawning.
use crate::context::Context;
use crate::effect::does;
use omega_proto::omega::{ConnectWifi, DisconnectWifi, action};

/// Ask NetworkManager to connect or disconnect the wireless device.
/// Completion acknowledges the request; `reading::Wifi` reports its outcome.
///
/// ```no_run
/// # async fn example(wifi: &omega::effect::WifiControl) -> omega::Result<()> {
/// wifi.connect("Home", "password").await?;
/// # Ok(()) }
/// ```
#[derive(Debug)]
pub struct WifiControl {
    context: Context,
}
does!(WifiControl, Network);
impl WifiControl {
    /// Connect to a visible open or personal Wi-Fi network. An empty password
    /// uses an existing saved profile for a secured network.
    pub fn connect(
        &self,
        ssid: impl Into<String>,
        password: impl Into<String>,
    ) -> crate::effect::Effect {
        self.act(action::Kind::ConnectWifi(ConnectWifi {
            ssid: ssid.into(),
            password: password.into(),
        }))
    }
    /// Disconnect Wi-Fi without disconnecting other devices.
    pub fn disconnect(&self) -> crate::effect::Effect {
        self.act(action::Kind::DisconnectWifi(DisconnectWifi {}))
    }
}
