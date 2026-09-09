//! The renderers omega ships, and the host shell they install into.
//!
//! A unit publishes a view tree; something has to draw it. That something is
//! QML running inside the host shell's own long-lived process, and it reads
//! the same wire format the daemon writes — protobuf's `intValue` arriving as
//! a *string* is one fact with a reader on each side of it.
//!
//! So a renderer and the daemon are one protocol in two halves, and they have
//! to be the same version. That is why the QML lives in this repository and
//! travels *inside* the binary rather than being copied out of a checkout:
//! the renderer a binary installs is, by construction, the one its daemon
//! speaks to. A stale copy stops being a mistake to be careful about and
//! becomes a state that cannot be reached.

mod host;
mod icons;
mod installed;
mod props;
mod renderer;

pub use host::HostShell;
pub use icons::Icons;
pub use installed::Installed;
pub use props::Props;
pub use renderer::{Asset, Renderer};
