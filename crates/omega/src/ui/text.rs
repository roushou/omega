//! Things made of glyphs.

use std::fmt::Display;

use crate::ui::node::{Node, Size};
use crate::ui::style::styled;
use omega_proto::Glyph;

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

/// A glyph from the shell's icon set.
///
/// [`Glyph`] is the set, so a name this build cannot draw is a compile error
/// rather than a word in somebody's bar. [`Icon::named`] is the way out for a
/// unit written against a richer shell.
///
#[derive(Debug, Clone)]
pub struct Icon {
    node: Node,
}

impl Icon {
    pub fn new(glyph: Glyph) -> Self {
        Self {
            node: Node::new("icon").text_prop("name", glyph.name()),
        }
    }

    /// An icon by a name this build does not know.
    ///
    /// The wire keeps `icon.name` a string so a shell meeting a name it has
    /// no glyph for draws the name itself — a legible word rather than a
    /// blank space. This is how a unit written for a richer shell reaches
    /// one, and it is deliberately the longer spelling: a name here is not
    /// checked by anything.
    pub fn named(name: impl Into<String>) -> Self {
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
