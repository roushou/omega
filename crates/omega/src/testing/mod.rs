//! Testing a plugin without a daemon.
//!
//! A widget is a function from state to a tree, so most of what can be wrong
//! with one is wrong before any socket exists. Build the plugin against the
//! state you want and look at what it drew:
//!
//! ```
//! use omega::testing::{Drawn, State};
//! use omega::reading::Battery;
//! use omega::ui::Text;
//! use omega::{Ui, Widget};
//!
//! #[derive(omega::Widget)]
//! struct Charge {
//!     battery: Battery,
//! }
//!
//! impl Widget for Charge {
//!     fn render(&self) -> Ui {
//!         Text::new(self.battery.charge()).into()
//!     }
//! }
//!
//! let drawn = Drawn::of::<Charge>(&State::new().battery(0.8, false));
//! assert_eq!(drawn.text(), "80%");
//! ```
//!
//! The rest — what a command answers, which instances a document hands a
//! widget, whether a plugin publishes at all — needs the protocol, so
//! [`TestDaemon`] speaks it over a `UnixStream::pair`. There is no listener,
//! no socket file, and no daemon: the test *is* the daemon.

mod called;
mod daemon;
mod drawn;
mod state;

pub use called::Called;
pub use daemon::{Published, TestDaemon, manifest_of};
pub use drawn::Drawn;
pub use state::State;

/// The topic payloads [`State::with`] takes, for the readings the shorthands
/// on `State` do not describe.
///
/// One per row of `omega-proto`'s topic table — the same set [`SystemTopic`]
/// closes over, because a topic a test cannot describe is a reading no widget
/// can be tested against.
///
/// [`State::battery`] says a charge and a direction, which is what most tests
/// mean. A test about the *time* left has to say the seconds, and that means
/// naming the state itself — so the types a unit is handed to build one are
/// here rather than in `omega-proto`, which a unit does not depend on.
///
/// ```
/// use omega::testing::{State, topic::BatteryState};
///
/// State::new().with(BatteryState {
///     level: 0.27,
///     charging: false,
///     seconds_to_empty: 6300,
///     seconds_to_full: 0,
/// });
/// ```
///
/// [`SystemTopic`]: omega_proto::SystemTopic
pub mod topic {
    pub use omega_proto::omega::{
        AudioState, BacklightState, BatteryState, BluetoothState, DiskState, IdleState, InputState,
        MainsState, MediaState, MonitorsState, NetworkState, PeripheralsState, PowerProfileState,
        SystemState, ThermalsState, ThroughputState, TimeState, UnitsState, VpnState, WifiState,
        WindowState, WorkspacesState,
    };
}
