//! Isolated development host, installed only in an ephemeral preview session.
use crate::Asset;

#[derive(Debug)]
pub struct Preview;
impl Preview {
    pub const FILES: &'static [Asset] = &[
        Asset {
            name: "preview/Viewport.qml",
            contents: include_str!("../shell/preview/Viewport.qml"),
        },
        Asset {
            name: "shell.qml",
            contents: include_str!("../shell/preview/shell.qml"),
        },
        Asset {
            name: "preview/Preview.qml",
            contents: include_str!("../shell/preview/Preview.qml"),
        },
    ];
}
