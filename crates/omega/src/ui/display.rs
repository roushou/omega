//! Progress indicators, graphs, and images.

use crate::ui::node::Node;
use crate::ui::style::styled;
use crate::units::Percent;

/// A read-only percentage bar. Use [`Slider`](crate::ui::Slider) for input.
#[derive(Debug, Clone)]
pub struct Progress {
    node: Node,
}

impl Progress {
    pub fn new(filled: Percent) -> Self {
        Self {
            node: Node::new("progress").fraction("value", filled.fraction()),
        }
    }
}

styled!(Progress);

/// A line graph of caller-supplied numeric samples.
/// Uses the minimum and maximum samples as its vertical scale by default.
/// Set a fixed range for bounded values such as percentages. The graph does
/// not collect history; retain samples in your model or record.
///
/// ```
/// use omega::ui::Graph;
/// let graph = Graph::new([3.0, 4.0, 2.0]).range(0.0, 100.0);
/// ```
#[derive(Debug, Clone)]
pub struct Graph {
    node: Node,
}

impl Graph {
    /// Create a graph from points ordered oldest to newest, left to right.
    pub fn new(points: impl IntoIterator<Item = f64>) -> Self {
        Self {
            node: Node::new("graph").fractions("points", points.into_iter().collect()),
        }
    }

    /// Set a fixed vertical range. If `high <= low`, automatic scaling is used.
    pub fn range(mut self, low: f64, high: f64) -> Self {
        self.node = self.node.fraction("low", low).fraction("high", high);
        self
    }
}

styled!(Graph);

/// Display an image from an absolute path, local `file:` URI, or `data:` URI.
/// Unsupported sources, including HTTP URLs, render no image.
/// Set [`width`](Self::width) and [`height`](Self::height) to override its natural size.
///
/// ```
/// use omega::ui::Image;
/// let art = Image::new("/home/me/.cache/art.png").width(64).height(64);
/// ```
#[derive(Debug, Clone)]
pub struct Image {
    node: Node,
}

impl Image {
    /// A freedesktop theme icon name or absolute file path. Resolution belongs
    /// to the host; an unavailable icon falls back to the host's application icon.
    ///
    /// ```
    /// let icon = omega::ui::Image::icon("org.gnome.Nautilus").width(32).height(32);
    /// ```
    pub fn icon(name: &str) -> Self {
        if name.starts_with('/') {
            return Self::new(name);
        }
        let mut node = Node::new("image");
        if !name.is_empty() && !name.contains(['/', '\\']) && !name.chars().any(char::is_control) {
            node = node.text_prop("source", format!("icon://{name}"));
        }
        Self { node }
    }

    pub fn new(source: impl Into<String>) -> Self {
        let source = source.into();
        let mut node = Node::new("image");
        if Self::is_local(&source) {
            node = node.text_prop("source", source);
        }
        Self { node }
    }

    /// Allow absolute paths, local `file:` URIs, and inline `data:` sources only.
    fn is_local(source: &str) -> bool {
        source.starts_with('/') || source.starts_with("file:/") || source.starts_with("data:")
    }
}

styled!(Image);

/// A small count pill. Hide when the count is zero with
/// [`hidden_when_zero`](Self::hidden_when_zero); tone carries its meaning.
#[derive(Debug, Clone)]
pub struct Badge {
    node: Node,
}

impl Badge {
    pub fn new(count: u32) -> Self {
        Self {
            node: Node::new("badge").number("count", count),
        }
    }

    /// Draw nothing when the count is zero.
    pub fn hidden_when_zero(mut self) -> Self {
        self.node = self.node.flag("hidden_when_zero", true);
        self
    }
}

styled!(Badge);

/// A single key or chord drawn as a keycap, for shortcuts and hints.
#[derive(Debug, Clone)]
pub struct Keycap {
    node: Node,
}

impl Keycap {
    pub fn new(label: impl std::fmt::Display) -> Self {
        Self {
            node: Node::new("keycap").text_prop("label", label.to_string()),
        }
    }
}

styled!(Keycap);

/// An empty, loading, or failed state with an optional glyph, a heading, and
/// a message. It is a display, not a control; pair it with a button for a
/// recovery action.
///
/// ```
/// use omega::ui::{EmptyState, Glyph};
/// let none = EmptyState::new("No devices").icon(Glyph::Bluetooth);
/// ```
#[derive(Debug, Clone)]
pub struct EmptyState {
    node: Node,
}

impl EmptyState {
    pub fn new(title: impl std::fmt::Display) -> Self {
        Self {
            node: Node::new("status").text_prop("title", title.to_string()),
        }
    }

    /// Supporting text under the title.
    pub fn message(mut self, message: impl std::fmt::Display) -> Self {
        self.node = self.node.text_prop("message", message.to_string());
        self
    }

    /// A [`Glyph`](crate::ui::Glyph) name to draw above the title.
    pub fn icon(mut self, glyph: crate::ui::Glyph) -> Self {
        self.node = self.node.text_prop("icon", glyph.name());
        self
    }
}

styled!(EmptyState);
