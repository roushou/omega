//! Render surfaces, invoke commands, and inspect effects using isolated fixtures.
//! Use [`Drawn`] for view assertions, [`Called`] for command results, and
//! [`TestDaemon`] for protocol-level tests. No live desktop services are required.
//!
//! ```
//! use omega::testing::{Drawn, State};
//! use omega::platform::power::Battery;
//! use omega::ui::Text;
//! use omega::{View, Surface};
//!
//! #[derive(omega::Surface)]
//! struct Charge {
//!     battery: Battery,
//! }
//!
//! impl Surface for Charge {
//!     type Model = ();
//!     type Message = std::convert::Infallible;
//!     type Effects = ();
//!     fn update(&self, _: &mut (), message: Self::Message, _: &()) -> omega::surface::Task<Self::Message> {
//!         match message {}
//!     }
//!
//!     fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View {
//!         Text::new(self.battery.charge()).into()
//!     }
//! }
//!
//! let drawn = Drawn::of::<Charge>(&State::new().battery(0.8, false)).unwrap();
//! assert_eq!(drawn.text(), "80%");
//! ```
//!

mod called;
mod daemon;
mod drawn;
mod state;

/// A system topic to mark absent in a test fixture.
pub use omega_proto::SystemTopic;

pub use called::Called;
pub use daemon::{Published, TestDaemon, manifest_of};
pub use drawn::Drawn;
pub use state::State;

/// Raw topic payloads accepted by [`State::with`]. Use these when the
/// fixture helpers do not expose the fields needed by a test.
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
        ActiveNotification, Application, ApplicationsState, AudioState, AudioStream,
        AudioStreamsState, BacklightState, BatteryState, BluetoothDevice, BluetoothState,
        ClipboardState, DiskState, Fan, IdleState, InputState, MainsState, MediaState,
        MonitorsState, Mount, NetworkState, NotificationsState, PeripheralsState, PluginsState,
        PowerProfileState, Sensor, SystemState, ThermalsState, ThroughputState, TimeState,
        VpnState, WifiState, WindowState, WorkspacesState,
    };
}

mod surface;
pub use surface::{CapturedEffect, SurfaceHarness};

/// Inspect the operations captured by fixture effect queues and supply refusals.
pub mod operation {
    pub use omega_proto::Refusal;
    pub use omega_proto::omega::{
        ErrorCode, PresentationAction, action::Kind as Action, invoke::Op as Operation,
    };
}

mod storage;
pub use storage::{
    StorageInsert, StorageQuery, StorageRead, StorageRemove, StorageReplace, Stored,
};

mod command;
pub use command::{CommandCall, CommandList};
