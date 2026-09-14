//! The entry point for a supervised renderer host, installed alongside `Core`.
use crate::Asset;

#[derive(Debug)]
pub struct Desktop;
impl Desktop {
    pub const FILES: &'static [Asset] = &[Asset {
        name: "shell.qml",
        contents: include_str!("../shell/desktop/shell.qml"),
    }];
}
