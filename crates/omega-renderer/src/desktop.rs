//! The entry point for a supervised renderer host, installed alongside `Core`.
use crate::Asset;

#[derive(Debug)]
pub struct Desktop;
impl Desktop {
    pub fn build() -> crate::Build {
        crate::Build::of(
            env!("CARGO_PKG_VERSION"),
            crate::Core::FILES.iter().chain(Self::FILES),
        )
    }
    pub const FILES: &'static [Asset] = &[Asset {
        name: "shell.qml",
        contents: include_str!("../shell/desktop/shell.qml"),
    }];
}
