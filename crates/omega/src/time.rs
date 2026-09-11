//! The machine’s local clock and calendar.
//!
//! Holding a state handle subscribes the plugin to its topics. Control handles
//! belong on commands or reactions; a widget cannot hold them.
//!
//! ```
//! use omega::time::Clock;
//!
//! #[derive(omega::Widget)]
//! struct Indicator {
//!     clock: Clock,
//! }
//! ```

pub use crate::reading::{Clock, Weekday};
