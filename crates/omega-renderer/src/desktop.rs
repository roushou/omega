//! The entry point for a supervised renderer host, installed alongside `Core`.
use crate::Asset;

#[derive(Debug)]
pub struct Desktop;
impl Desktop {
    pub fn build() -> crate::Build {
        Self::build_with_theme(Self::THEME)
    }

    /// Fingerprint the standalone host with its selected theme component.
    /// Install `theme` as [`Self::THEME_FILE`] alongside [`Self::FILES`]. The
    /// component must derive from the shared `Theme` contract.
    pub fn build_with_theme(theme: &str) -> crate::Build {
        crate::Build::of_sources(
            env!("CARGO_PKG_VERSION"),
            crate::Core::FILES
                .iter()
                .chain(Self::FILES)
                .map(|asset| (asset.name, asset.contents))
                .chain([(Self::THEME_FILE, theme)]),
        )
    }
    pub const THEME_FILE: &'static str = "DesktopTheme.qml";
    pub const THEME: &'static str = include_str!("../shell/desktop/DesktopTheme.qml");
    pub const FILES: &'static [Asset] = &[Asset {
        name: "shell.qml",
        contents: include_str!("../shell/desktop/shell.qml"),
    }];
}
