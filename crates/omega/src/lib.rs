//! Write a plugin.
//!
//! A plugin is a program the daemon runs. It declares what it needs by
//! holding it — a `Battery` field is a view onto the battery topic and the
//! permission to read it — and offers what it does by registering it. There
//! is no manifest to keep in step with the code, because the manifest *is*
//! the code: `omega build` compiles a plugin and asks it what it declares.
//!
//! ```no_run
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
//!     fn render(&self) -> View {
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
//!     omega::plugin!().surface(Charge).run()
//! }
//! ```
//!
//! Find state and control handles by domain: [`platform::audio`], [`platform::power`], [`platform::network`],
//! [`platform::bluetooth`], [`platform::desktop`], [`platform::time`], [`platform::system`], and [`platform::session`].
//! [`platform::notification`] and [`platform::process`] provide notifications and process execution.
//! Build views with [`ui`], supply settings with [`config`], and share plugin
//! memory through [`record`]. Shared units such as [`Percent`] live at the root.
//!
//! A [`Surface`] renders from readings; a [`StatefulSurface`] adds a local model
//! and serialized messages with separate behavior effects. [`Command`] endpoints
//! are explicitly callable, and [`Reaction`] runs when an event occurs.
//! Render declarations accept only reading handles. Stateful behavior receives
//! effects separately, keeping them out of ordinary render-side wiring.

#[cfg(test)]
extern crate self as omega;

pub mod command;
mod error;
pub mod platform;
pub mod plugin;
pub mod reaction;
mod runtime;
pub mod surface;
mod units;
mod wiring;

pub mod effect;
pub mod record;
pub mod testing;
pub mod ui;

pub use command::{Args, Command, Input};
pub use error::{Error, Result};
pub use plugin::Plugin;
pub use reaction::Reaction;
pub use surface::{StatefulSurface, Surface};

/// What `Surface::render` hands back. Part of the surface's contract, so it
/// lives beside the trait rather than with the nodes it is built from.
pub use ui::{Ui, View};

/// What a reaction is called with.
pub use omega_proto::omega::{Event, EventKind};

/// The units a reading is in. At the root because they cross every
/// boundary — a handle hands one back, a node draws one, a setting is
/// compared against one.
pub use units::{Bytes, Percent, Rate, Remaining, Temperature, Uptime};

pub use omega_derive::{Command, Config, Effects, Form, Input, Reaction, Surface, UnitState};

/// Settings: the values a document hands an instance, and what
/// `#[derive(Config)]` implements to read them.
pub mod config {
    pub use omega_proto::{Fields, FromValue, IntoValue, Values};
}

/// Internals the derives expand into. Not a stable surface: write
/// `#[derive(Surface)]`, not this.
#[doc(hidden)]
pub mod internal {
    pub use crate::Command;
    pub use crate::command::CommandName;
    pub use crate::command::CommandRef;
    pub use crate::command::Input;
    pub use crate::record::UnitState;
    pub use crate::runtime::context::Context;
    pub use crate::surface::{SurfaceIdentity, SurfaceRef};
    pub use crate::ui::{Bind, Field, FormInput};
    pub use crate::wiring::Wired;
    pub use crate::wiring::{Does, Reads, Wiring};
    pub use crate::{Args, Error};
    pub use omega_proto::omega::Capability;
    pub use omega_proto::omega::Value;
    pub use omega_proto::{Fields, FromValue, IntoValue, SystemTopic, Values};
}
