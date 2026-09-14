//! Text, glyph icons, and section headings.

use std::fmt::Display;

use crate::ui::node::{Node, Size};
use crate::ui::style::styled;
use omega_proto::Glyph;

/// Display text from any value implementing [`Display`].
/// Measurement types format automatically, for example `Text::new(Percent::whole(80))`.
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

    /// Set the semantic text size.
    pub fn size(mut self, size: Size) -> Self {
        self.node = self.node.text_prop("size", size.as_str());
        self
    }
}

styled!(Text);

/// A glyph icon. Use [`Glyph`] for supported names or [`Icon::named`]
/// for a name resolved by the renderer.
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

    /// Create an icon from a renderer-resolved name.
    /// Names are not checked at compile time. Unknown names are displayed as text.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            node: Node::new("icon").text_prop("name", name),
        }
    }

    /// Set the glyph's semantic size. Defaults to the theme's icon size.
    pub fn size(mut self, size: Size) -> Self {
        self.node = self.node.text_prop("size", size.as_str());
        self
    }
}

styled!(Icon);

/// A section heading using the theme's heading weight and size.
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
