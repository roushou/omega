//! Write a plugin.
//!
//! A plugin is a program the daemon runs. It declares what it needs by
//! holding it — a `Battery` field is a view onto the battery topic and the
//! permission to read it — and offers what it does by registering it. There
//! is no manifest to keep in step with the code, because the manifest *is*
//! the code: `omega build` compiles a plugin and asks it what it declares.
//!
//! ```no_run
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

#[cfg(test)]
extern crate self as omega;

mod context;
mod error;
mod mirror;
mod plugin;
mod registry;
mod runtime;
mod surface;
mod units;

pub mod composite;
pub mod effect;
pub mod reading;
pub mod record;
pub mod testing;
pub mod ui;
pub mod wiring;

// ---- what a plugin is -------------------------------------------------
//
// The root is the vocabulary every unit uses whatever it does: the three
// surfaces, what they are called with, what they answer, and the readings
// that pass through both.
//
// Everything else is in a module named for the *kind of field* it holds,
// because that is the distinction `wiring` already makes and the one that
// decides what a surface may hold:
//
// - `reading`   — what is true of the machine. Safe on anything.
// - `composite` — a reading that takes more than one topic to answer.
// - `record`  — what this unit remembers. Reading one is; writing is not.
// - `effect`  — what this unit can change. Never on a widget.
// - `ui`      — what a widget draws with. Not a field at all.
//
// `state` was none of these. It named the plane the value lives in rather
// than the thing being imported, which is why `omega::reading::Battery` read
// oddly where `omega::ui::Row` does not.

pub use error::{Error, Result};
pub use plugin::Plugin;
pub use surface::{Args, Command, Reaction, Widget, Wired};

/// What `Widget::render` hands back. Part of the surface's contract, so it
/// lives beside the trait rather than with the nodes it is built from.
pub use ui::Ui;

/// What a reaction is called with.
pub use omega_proto::omega::{Event, EventKind};

/// The units a reading is in. At the root because they cross every
/// boundary — a handle hands one back, a node draws one, a setting is
/// compared against one.
pub use units::{Bytes, Percent, Rate, Remaining, Temperature, Uptime};

pub use omega_derive::{Command, Config, Reaction, UnitState, Widget};

/// Settings: the values a document hands an instance, and what
/// `#[derive(Config)]` implements to read them.
pub mod config {
    pub use omega_proto::{Fields, FromValue, IntoValue, Values};
}

/// Internals the derives expand into. Not a stable surface: write
/// `#[derive(Widget)]`, not this.
#[doc(hidden)]
pub mod internal {
    pub use crate::context::Context;
    pub use crate::record::UnitState;
    pub use crate::surface::Wired;
    pub use crate::wiring::{Does, Reads, Wiring};
    pub use omega_proto::omega::Capability;
    pub use omega_proto::omega::Value;
    pub use omega_proto::{Fields, FromValue, IntoValue, SystemTopic, Values};
}
