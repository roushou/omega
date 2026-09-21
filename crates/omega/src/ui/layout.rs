//! Rows, columns, grids, separators, and spacing.

use crate::ui::node::{Align, Node};
use crate::ui::style::styled;

/// A horizontal or vertical sequence of children.
/// Stacks inside columns fill the column width. Rows distribute extra width
/// to children using [`fill_width`](Self::fill_width) or [`Spacer`].
///
/// ```
/// use omega::ui::{Column, Progress, Row, Text};
/// use omega::Percent;
/// let charge = Percent::whole(27);
/// let view = Column::new()
///     .child(Row::new().child(Text::new(charge).bold()))
///     .child(Progress::new(charge).fill_width());
/// ```
#[derive(Debug, Clone)]
pub struct Stack {
    node: Node,
}

impl Stack {
    fn along(align: Align) -> Self {
        Self {
            node: Node::new("stack").text_prop("align", align.as_str()),
        }
    }

    /// Space between children.
    pub fn gap(mut self, gap: u32) -> Self {
        self.node = self.node.number("gap", gap);
        self
    }

    pub fn child(mut self, child: impl Into<crate::View>) -> Self {
        self.node = self.node.child(child);
        self
    }

    /// Append children from an iterator. Assign stable
    /// [`keys`](crate::ui::Text::key) to children that can be reordered or removed.
    pub fn children<C: Into<crate::View>>(mut self, children: impl IntoIterator<Item = C>) -> Self {
        for child in children {
            self.node = self.node.child(child);
        }
        self
    }
}

styled!(Stack);

/// Construct a horizontal [`Stack`].
#[derive(Debug)]
pub struct Row;

impl Row {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Stack {
        Stack::along(Align::Row)
    }
}

/// Construct a vertical [`Stack`].
#[derive(Debug)]
pub struct Column;

impl Column {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Stack {
        Stack::along(Align::Column)
    }
}

/// A separator perpendicular to its parent layout: horizontal in a column
/// and vertical in a row. Fills the parent's cross-axis.
#[derive(Debug, Clone)]
pub struct Separator {
    node: Node,
}

impl Separator {
    pub fn new() -> Self {
        Self {
            node: Node::new("separator"),
        }
    }
}

impl Default for Separator {
    fn default() -> Self {
        Self::new()
    }
}

styled!(Separator);

/// Empty layout space. Expands along the parent layout's axis by default;
/// set [`width`](Self::width) or [`height`](Self::height) for fixed spacing.
///
/// ```
/// use omega::ui::{Row, Spacer, Text};
/// let row = Row::new()
///     .child(Text::new("Headphones"))
///     .child(Spacer::new())
///     .child(Text::new("40%").muted());
/// ```
#[derive(Debug, Clone)]
pub struct Spacer {
    node: Node,
}

impl Spacer {
    /// Create flexible empty space. Set a width or height for fixed spacing.
    pub fn new() -> Self {
        Self {
            node: Node::new("spacer").flag("fill", true),
        }
    }
}

impl Default for Spacer {
    fn default() -> Self {
        Self::new()
    }
}

styled!(Spacer);

/// A grid with a fixed number of columns and aligned cells.
#[derive(Debug, Clone)]
pub struct Grid {
    node: Node,
}

impl Grid {
    /// Create a grid with the given column count. Zero is treated as one.
    pub fn new(columns: u32) -> Self {
        Self {
            node: Node::new("grid").number("columns", columns.max(1)),
        }
    }

    /// Set horizontal and vertical spacing between cells.
    pub fn gap(mut self, gap: u32) -> Self {
        self.node = self.node.number("gap", gap);
        self
    }

    pub fn child(mut self, child: impl Into<crate::View>) -> Self {
        self.node = self.node.child(child);
        self
    }

    pub fn children<C: Into<crate::View>>(mut self, children: impl IntoIterator<Item = C>) -> Self {
        for child in children {
            self.node = self.node.child(child);
        }
        self
    }
}

styled!(Grid);

/// Children in a scrollable viewport. Set [`height`](Self::height) to bound the
/// viewport; without one it sizes to its children and does not scroll.
#[derive(Debug, Clone)]
pub struct Scroll {
    node: Node,
}

impl Scroll {
    pub fn new() -> Self {
        Self {
            node: Node::new("scroll"),
        }
    }

    pub fn child(mut self, child: impl Into<crate::View>) -> Self {
        self.node = self.node.child(child);
        self
    }

    pub fn children<C: Into<crate::View>>(mut self, children: impl IntoIterator<Item = C>) -> Self {
        for child in children {
            self.node = self.node.child(child);
        }
        self
    }
}

impl Default for Scroll {
    fn default() -> Self {
        Self::new()
    }
}

styled!(Scroll);
