//! Things made of glyphs.

use std::fmt::Display;

use crate::ui::node::{Node, Size};
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

    /// What this text is for, which is what decides how big it is.
    pub fn size(mut self, size: Size) -> Self {
        self.node = self.node.text_prop("size", size.as_str());
        self
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

    /// Draw the glyph at a text role's size, for an icon that has to hold
    /// its own beside a figure. Unset, it is whatever the shell draws icons
    /// at, which is already a little larger than body text.
    pub fn size(mut self, size: Size) -> Self {
        self.node = self.node.text_prop("size", size.as_str());
        self
    }
}

styled!(Icon);

/// What a section of a panel is called.
///
/// A [`Text`] with the weight and spacing a shell gives its section titles,
/// so a panel written here looks like the panels beside it without an author
/// choosing a size.
#[derive(Debug, Clone)]
pub struct Header {
    node: Node,
}

impl Header {
    pub fn new(text: impl Display) -> Self {
        Self {
            node: Node::new("header").text_prop("text", text.to_string()),
        }
    }
}

styled!(Header);
