//! Things made of glyphs.

use std::fmt::Display;

use crate::ui::node::Node;
use crate::ui::style::styled;

/// A run of text.
///
/// Takes anything that prints itself, which is why the units in this crate
/// do: `Text::new(battery.charge())` is `80%` with nothing to format.
#[derive(Debug, Clone)]
pub struct Text {
    node: Node,
}

impl Text {
    pub fn new(text: impl Display) -> Self {
        Self {
            node: Node::new("text").text_prop("text", text.to_string()),
        }
    }
}

styled!(Text);

/// A named icon, as the shell's icon set spells it.
///
/// The shell draws these as Nerd Font glyphs. A name it does not know is
/// drawn as the name itself, so a typo — or a unit written for a richer
/// shell — reads as a legible word rather than a blank space.
///
/// What `omega.view` draws today:
///
/// `battery`  `battery-full`  `battery-three-quarters`  `battery-half`
/// `battery-quarter`  `battery-empty`  `plug`  `power`
/// `wifi`  `globe`  `link`  `bluetooth`
/// `download`  `upload`  `volume`  `volume-up`
/// `volume-down`  `volume-off`  `headphones`  `microphone`
/// `microphone-off`  `music`  `cpu`  `thermometer`
/// `keyboard`  `camera`  `terminal`  `cog`
/// `clock`  `calendar`  `sun`  `moon`
/// `bell`  `warning`  `check`  `close`
/// `refresh`  `lock`  `search`  `star`
/// `heart`  `home`  `user`  `folder`
/// `envelope`  `trash`
#[derive(Debug, Clone)]
pub struct Icon {
    node: Node,
}

impl Icon {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            node: Node::new("icon").text_prop("name", name),
        }
    }
}

styled!(Icon);
