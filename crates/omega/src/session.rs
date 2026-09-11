//! Session activity, locking, and suspension.
//!
//! Holding a state handle subscribes the plugin to its topics. Control handles
//! belong on commands or reactions; a widget cannot hold them.
//!
//! ```
//! use omega::session::Idle;
//!
//! #[derive(omega::Widget)]
//! struct Indicator {
//!     idle: Idle,
//! }
//! ```

pub use crate::effect::session::Session;
pub use crate::reading::Idle;
