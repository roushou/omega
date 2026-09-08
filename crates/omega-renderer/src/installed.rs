//! What is on disk where a renderer belongs.

use std::path::PathBuf;

use crate::renderer::Renderer;

/// What is installed where a renderer belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    /// Nothing is there.
    Missing,
    /// A symlink to a checkout: somebody is working on the QML, and what the
    /// shell draws is whatever is in their tree.
    Linked(PathBuf),
    /// The files this binary carries, byte for byte.
    Current,
    /// Something else — an older install, or a hand edit. The version is what
    /// the copy on disk claims, when it claims one.
    Stale { version: Option<String> },
}

impl Installed {
    /// How this differs from what the binary carries, when it does — the one
    /// phrase `omega shell status` and `omega check` both report.
    ///
    /// The case worth spelling out is a copy that claims the right version
    /// and holds different bytes: "0.1.0 installed, this omega draws 0.1.0"
    /// reads as a bug in the check rather than a fact about the disk. The
    /// manifest is what a copy says of itself; the contents are the evidence.
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
