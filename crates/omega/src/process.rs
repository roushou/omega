//! External process execution.
//!
//! Holding [`Shell`] declares the capability to act. Use it on a command or
//! reaction; widgets cannot perform external actions.
//!
//! ```
//! use omega::process::Shell;
//!
//! #[derive(omega::Command)]
//! struct Action {
//!     control: Shell,
//! }
//! ```

pub use crate::effect::shell::Shell;
