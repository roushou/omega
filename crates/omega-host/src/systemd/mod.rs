//! Systemd service definitions, explicit file locations, and asynchronous manager operations.
//!
//! [`ServiceUnit`] renders a definition; [`Service`] binds a validated name and
//! file path to a [`Manager`]. File installation, manager state, and application
//! readiness are separate contracts. Callers choose environment discovery,
//! session dependencies, recovery publication, and activation ordering.

mod manager;
mod name;
mod service;
mod status;
mod unit;

pub use manager::{Manager, ManagerError, Scope};
pub use name::{NameError, UnitName};
pub use service::{Installed, Service, ServiceError};
pub use status::{ActiveState, Enablement, LoadState, Status, StatusError};
pub use unit::{ExecStart, Restart, ServiceUnit, UnitError};
