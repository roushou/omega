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
//!         match self.battery.is_charging() {
//!             true => Text::new(format!("{} charging", self.battery.charge())),
//!             false => Text::new(self.battery.charge()),
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
mod source;
mod state;
mod surface;
mod ui;
mod units;

pub mod testing;
pub mod wiring;

pub use effect::{Notification, Notify, Session, Shell, Volume};
pub use error::{Error, Result};
pub use plugin::Plugin;
pub use source::{Audio, Battery, Network};
pub use state::{Own, Topic, Watch};
pub use surface::{Answer, Args, Command, Reaction, Widget, Wired};
pub use ui::{Align, Button, Column, Icon, Node, Progress, Row, Stack, Text, Ui};
pub use units::{Percent, Remaining};

pub use omega_derive::{Command, Config, Reaction, Topic, Widget};

/// A struct that is a map of values: what `#[derive(Config)]` implements.
pub use omega_proto::{Fields, FromValue, IntoValue, Values};

/// The events a reaction can answer.
pub use omega_proto::omega::{Event, EventKind};

/// Internals the derives expand into. Not a stable surface: write
/// `#[derive(Widget)]`, not this.
#[doc(hidden)]
pub mod internal {
    pub use crate::context::Context;
    pub use crate::surface::Wired;
    pub use crate::wiring::{Does, Reads, Wiring};
    pub use omega_proto::omega::Capability;
    pub use omega_proto::{Fields, FromValue, IntoValue, SystemTopic, Values};
}
