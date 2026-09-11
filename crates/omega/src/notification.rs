//! Desktop notifications.
//!
//! Holding [`Notify`] declares the capability to act. Use it on a command or
//! reaction; widgets cannot perform external actions.
//!
//! ```
//! use omega::notification::Notify;
//!
//! #[derive(omega::Command)]
//! struct Action {
//!     control: Notify,
//! }
//! ```

pub use crate::effect::notify::{Notification, Notify};
