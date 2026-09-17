//! Build desktop surfaces, commands, and automations.
//!
//! Declare state and control dependencies as struct fields. Derives collect their
//! subscriptions and capabilities into the plugin manifest. Register surfaces,
//! commands, and reactions with [`Plugin`].
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
//!     type Model = ();
//!     type Message = std::convert::Infallible;
//!     type Effects = ();
//!     fn update(&self, _: &mut (), message: Self::Message, _: &()) -> omega::surface::Task<Self::Message> {
//!         match message {}
//!     }
//!
//!     fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View {
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
//! A [`Surface`] renders from readings and an instance-local model, with serialized
//! messages and separate behavior effects. [`Command`] endpoints
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

/// Host-independent logical keys, chords and composable keymaps.
pub use omega_keyboard as keyboard;

pub use command::{Args, Command, Input};
pub use error::{Error, Result};
pub use plugin::Plugin;
pub use reaction::Reaction;
pub use surface::Surface;

/// Declarative UI content returned by surfaces and components.
pub use ui::{Ui, View};

/// State-transition events delivered to reactions.
pub use omega_proto::omega::{Event, EventKind};

/// Measurement types with display formatting and unit conversions.
pub use units::{Bytes, Percent, Rate, Remaining, Temperature, Uptime};

pub use omega_derive::{Command, Config, Effects, Form, Input, PluginState, Reaction, Surface};

/// Construction settings and typed field serialization. Derive `Config` to read settings.
pub mod config {
    pub use omega_proto::{Fields, FromValue, IntoValue, Values};
}

/// Macro implementation support. Not a stable public API.
#[doc(hidden)]
pub mod internal {
    pub use crate::Command;
    pub use crate::command::CommandName;
    pub use crate::command::CommandRef;
    pub use crate::command::Input;
    pub use crate::record::PluginState;
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
