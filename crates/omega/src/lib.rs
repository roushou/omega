//! Write a plugin.
//!
//! A plugin is a program the daemon runs. It declares what it needs by
//! holding it — a `Battery` field is a view onto the battery topic and the
//! permission to read it — and offers what it does by registering it. There
//! is no manifest to keep in step with the code, because the manifest *is*
//! the code: `omega build` compiles a plugin and asks it what it declares.
//!
//! ```no_run
//! use omega::{Battery, Text, Ui, Widget};
//!
//! #[derive(omega::Widget)]
//! struct Charge {
//!     battery: Battery,
//! }
//!
//! impl Widget for Charge {
//!     fn render(&self) -> Ui {
//!         if self.battery.is_charging() {
//!             Text::new(format!("{} charging", self.battery.charge()))
//!         } else {
//!             Text::new(self.battery.charge())
//!         }
//!         .into()
//!     }
//! }
//!
//! fn main() -> omega::Result<()> {
//!     omega::plugin!().widget::<Charge>().run()
//! }
//! ```
//!
//! Three kinds of surface, and which one you are writing decides what you may
//! hold. A [`Widget`] renders, so it may hold state and nothing else — an
//! effect there would fire on every change the machine reports. A [`Command`]
//! is asked to do something, and a [`Reaction`] runs when something happened;
//! both may hold effects, because both run when there is a reason to.

mod context;
mod effect;
mod error;
mod mirror;
mod plugin;
mod registry;
mod runtime;
mod state;
mod surface;
mod ui;
mod units;

pub mod testing;
pub mod wiring;

pub use effect::{Brightness, Notification, Notify, Session, Shell, Volume};
pub use error::{Error, Result};
pub use plugin::Plugin;
pub use state::{
    AccessPoint, Audio, Backlight, Battery, Bluetooth, Clock, Disk, Idle, Input, Media, Monitors,
    Network, Own, Peripherals, Power, System, Topic, Units, Vpn, Watch, Weekday, Wifi, Window,
    Workspaces,
};
pub use surface::{Answer, Args, Command, Reaction, Widget, Wired};
pub use ui::{
    Align, Bind, Button, Column, Field, Graph, Grid, Group, Header, Icon, Image, List, Node,
    Progress, Row, Separator, Slider, Spacer, Stack, Text, Toggle, Ui,
};
pub use units::{Percent, Remaining};

pub use omega_derive::{Command, Config, Reaction, Topic, Widget};

/// A struct that is a map of values: what `#[derive(Config)]` implements.
pub use omega_proto::{Fields, FromValue, IntoValue, Values};

/// The events a reaction can answer.
pub use omega_proto::omega::{Event, EventKind};

/// Naming a topic, for a test that describes a machine which has none of it.
/// Reading one is a field like [`Battery`], never this.
pub use omega_proto::SystemTopic;

/// Internals the derives expand into. Not a stable surface: write
/// `#[derive(Widget)]`, not this.
#[doc(hidden)]
pub mod internal {
    pub use crate::context::Context;
    pub use crate::surface::Wired;
    pub use crate::wiring::{Does, Reads, Wiring};
    pub use omega_proto::omega::Capability;
    pub use omega_proto::omega::Value;
    pub use omega_proto::{Fields, FromValue, IntoValue, SystemTopic, Values};
}
