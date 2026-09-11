//! The configuration plane.
//!
//! A machine's desired state is a [`StateDocument`]: what should be true, not
//! what to do about it. The config workspace's `system/` crate builds one and
//! emits it; `omega build` stages it; the daemon's reconciler converges the
//! machine toward it.
//!
//! This crate owns all three sides of that: the authoring API a `system/`
//! crate writes against, the canonical file the document is stored as, and
//! the errors either can produce.

mod document;
mod error;
mod file;
mod keys;
mod validation;

pub use document::{Actions, Bars, Document, Host, Keybinds, Modules, Schedules, Settings, Units};
pub use error::DocumentError;
pub use file::DocumentFile;
pub use keys::Key;
pub use validation::{DocumentValidation, ValidationError};

pub use omega_proto::omega::StateDocument;

pub use omega_proto::Cadence;
/// The vocabulary a document is written in.
///
/// Re-exported so a `system/` crate declares one dependency and never names
/// the protocol: a config says what the machine should be, and which wire
/// types carry that is not its business.
pub use omega_proto::omega::{
    Action, Bar, Edge, Keybind, Modifier, Module, Schedule, Setting, UnitRef, Value, value,
};
