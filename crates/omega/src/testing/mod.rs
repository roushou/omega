//! Testing a plugin without a daemon.
//!
//! A widget is a function from state to a tree, so most of what can be wrong
//! with one is wrong before any socket exists. Build the plugin against the
//! state you want and look at what it drew:
//!
//! ```
//! use omega::testing::{Drawn, State};
//! use omega::state::Battery;
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
//!         Text::new(self.battery.charge()).into()
//!     }
//! }
//!
//! let drawn = Drawn::of::<Charge>(&State::new().battery(0.8, false));
//! assert_eq!(drawn.text(), "80%");
//! ```
//!
//! The rest — what a command answers, which instances a document hands a
//! widget, whether a plugin publishes at all — needs the protocol, so
//! [`TestDaemon`] speaks it over a `UnixStream::pair`. There is no listener,
//! no socket file, and no daemon: the test *is* the daemon.

mod called;
mod daemon;
mod drawn;
mod state;

pub use called::Called;
pub use daemon::{Published, TestDaemon, manifest_of};
pub use drawn::Drawn;
pub use state::State;
