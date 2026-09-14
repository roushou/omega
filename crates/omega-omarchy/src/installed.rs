//! What is on disk where a renderer belongs.

use std::path::PathBuf;

use crate::renderer::Renderer;

/// What is installed where a renderer belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    /// Nothing is there.
    Missing,
    /// Renderer files symlinked to a local checkout.
    Linked(PathBuf),
    /// The files this binary carries, byte for byte.
    Current,
    /// An installed renderer whose version or contents differ from the embedded files.
    Stale { version: Option<String> },
}

impl Installed {
    /// Describe version or content differences from the embedded renderer.
    pub fn difference(&self) -> Option<String> {
        let Self::Stale { version } = self else {
            return None;
        };
        Some(match version.as_deref() {
            Some(found) if found == Renderer::VERSION => "edited since it was installed".to_owned(),
            Some(found) => format!("{found} installed, this omega draws {}", Renderer::VERSION),
            None => "no manifest omega can read".to_owned(),
        })
    }
}
