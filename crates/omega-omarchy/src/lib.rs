//! Omarchy shell authoring and host integration for Omega.

mod desktop;
pub use desktop::DesktopRenderer;

mod host;
pub mod shell;
mod validation;

pub use host::HostShell;
pub use validation::DocumentValidation;

mod installed;
mod renderer;
pub use installed::Installed;
pub use renderer::Renderer;

pub mod installation;
