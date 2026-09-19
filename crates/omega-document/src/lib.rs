//! Build and serialize desktop desired state from the configuration's `system/` crate.
//! Use [`Document`] to configure plugins, schedules, and presentations.
//! Host integrations contribute declarations through [`DocumentExtension`].

mod document;
mod error;
mod extension;
mod file;
mod keys;
pub use extension::DocumentExtension;
mod validation;

pub use document::{
    Actions, Bars, Document, Host, Keybinds, Modules, Plugins, Schedules, Settings,
};
pub use error::{Error, Result};
pub use file::{DocumentError, DocumentFile};
pub use keys::Key;
pub use validation::{DocumentValidation, ValidationError};

pub use omega_proto::omega::StateDocument;

pub use omega_proto::Cadence;

/// Protocol types used by the document builders.
pub use omega_proto::omega::{
    Action, Bar, Edge, Keybind, Modifier, Module, PluginRef, Schedule, Setting, Value, value,
};

pub mod presentations;
pub use presentations::Presentations;
