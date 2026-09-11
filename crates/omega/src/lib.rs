//! Write a plugin.
//!
//! A plugin is a program the daemon runs. It declares what it needs by
//! holding it — a `Battery` field is a view onto the battery topic and the
//! permission to read it — and offers what it does by registering it. There
//! is no manifest to keep in step with the code, because the manifest *is*
//! the code: `omega build` compiles a plugin and asks it what it declares.
//!
//! ```no_run
//! use omega::power::Battery;
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
//! Find state and control handles by domain: [`audio`], [`power`], [`network`],
//! [`bluetooth`], [`desktop`], [`time`], [`system`], and [`session`].
//! [`notification`] and [`process`] provide notifications and process execution.
//! Build views with [`ui`], supply settings with [`config`], and share plugin
//! memory through [`record`]. Shared units such as [`Percent`] live at the root.
//!
//! Three kinds of surface, and which one you are writing decides what you may
//! hold. A [`Widget`] renders, so it may hold state and nothing else — an
//! effect there would fire on every change the machine reports. A [`Command`]
//! is asked to do something, and a [`Reaction`] runs when something happened;
//! both may hold effects, because both run when there is a reason to.

#[cfg(test)]
extern crate self as omega;

mod composite;
mod context;
mod error;
mod input;
mod mirror;
mod plugin;
mod reading;
mod registry;
mod runtime;
mod surface;
mod units;
mod wiring;

pub mod audio;
pub mod bluetooth;
pub mod desktop;
pub mod effect;
pub mod network;
pub mod notification;
pub mod power;
pub mod process;
pub mod record;
pub mod session;
pub mod system;
pub mod testing;
pub mod time;
pub mod ui;

pub use error::{Error, Result};
pub use input::Input;
pub use plugin::Plugin;
pub use surface::{Args, Command, Reaction, Widget};

/// What `Widget::render` hands back. Part of the surface's contract, so it
/// lives beside the trait rather than with the nodes it is built from.
pub use ui::Ui;

/// What a reaction is called with.
pub use omega_proto::omega::{Event, EventKind};

/// The units a reading is in. At the root because they cross every
/// boundary — a handle hands one back, a node draws one, a setting is
/// compared against one.
pub use units::{Bytes, Percent, Rate, Remaining, Temperature, Uptime};

pub use omega_derive::{Command, Config, Form, Input, Reaction, UnitState, Widget};

/// Settings: the values a document hands an instance, and what
/// `#[derive(Config)]` implements to read them.
pub mod config {
    pub use omega_proto::{Fields, FromValue, IntoValue, Values};
}

/// Internals the derives expand into. Not a stable surface: write
/// `#[derive(Widget)]`, not this.
#[doc(hidden)]
pub mod internal {
    pub use crate::Command;
    pub use crate::context::Context;
    pub use crate::input::Input;
    pub use crate::record::UnitState;
    pub use crate::surface::Wired;
    pub use crate::ui::bind::CommandName;
    pub use crate::ui::{Bind, CommandRef, Field, FormInput};
    pub use crate::wiring::{Does, Reads, Wiring};
    pub use crate::{Args, Error};
    pub use omega_proto::omega::Capability;
    pub use omega_proto::omega::Value;
    pub use omega_proto::{Fields, FromValue, IntoValue, SystemTopic, Values};
}
