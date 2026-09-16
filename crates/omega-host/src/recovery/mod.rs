//! Durable records for explicitly recoverable changes.
//!
//! A change defines how to observe, apply, and compensate its own effects. The
//! store records intent before effects and completion afterward. An interrupted
//! attempt is inspected, never blindly replayed. Recovery is explicit; dropping
//! a handle only releases its lock. Callers coordinate services before restoring
//! files they may still be using.

mod change;
mod installation;
mod policy;
mod record;
mod replacement;
mod saved;
mod store;

pub use change::{Change, Observation};
pub use installation::{InstalledReplacement, RetainedChange};
pub use record::{ChangeId, Receipt, State};
pub use replacement::{Replacement, Snapshot};
pub use saved::{RecoveryError, SavedChange};
pub use store::RecoveryStore;

#[cfg(test)]
mod tests;
