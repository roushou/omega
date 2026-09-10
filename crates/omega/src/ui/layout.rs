//! Where things go.

use crate::ui::node::{Align, Node};
use crate::ui::style::styled;

/// Children in a line.
///
/// **A stack inside a column is as wide as that column.** A block inside a
/// block is as wide as the block, so the panel's shape comes from the panel
/// rather than from each row's longest word — which is what lets a [`fill`]
/// deeper down mean anything at all:
///
/// ```
/// # use omega::ui::{Column, Progress, Row, Text};
/// # use omega::Percent;
/// # let charge = Percent::whole(27);
/// Column::new()
///     .child(Row::new().child(Text::new(charge).bold()))
///     // Spans the panel, because the column it is in does.
///     .child(Progress::new(charge).fill());
/// ```
///
/// It costs nothing to look at: a row's slack collects at its trailing edge
/// rather than between its children, so a row that spans draws what a row
/// that hugs drew until something inside it asks for the room. Along the way
/// a stack *runs*, room is given only to a node that asked — [`fill`] in a
/// row, or a [`Spacer`].
///
/// [`fill`]: Stack::fill
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

    pub fn child(mut self, child: impl Into<Node>) -> Self {
        self.node = self.node.child(child);
        self
    }

    /// Every one of them, for a list built from data.
    ///
    /// Give each one a [`key`] when the items can move: the shell keeps the
    /// node it already built for a key rather than rebuilding it, so a
    /// control mid-interaction survives its neighbours reordering.
    ///
    /// [`key`]: crate::ui::Text::key
    pub fn children<C: Into<Node>>(mut self, children: impl IntoIterator<Item = C>) -> Self {
        for child in children {
            self.node = self.node.child(child);
        }
        self
    }
}

styled!(Stack);

/// Children left to right.
///
/// A direction, not a type: `Row::new()` builds a [`Stack`], so a helper that
/// returns one is written `fn header() -> Stack`.
#[derive(Debug)]
pub struct Row;

impl Row {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Stack {
        Stack::along(Align::Row)
    }
}

/// Children top to bottom.
///
/// A direction, not a type: `Column::new()` builds a [`Stack`], so a helper
/// that returns one is written `fn details() -> Stack`.
#[derive(Debug)]
pub struct Column;

impl Column {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Stack {
        Stack::along(Align::Column)
    }
}

/// A line between things.
///
/// Draws itself across whichever way its parent runs, so the same separator
/// is a rule in a column and a divider in a row. It spans that stack without
/// being told to: a rule that stopped short of the panel it divides is not a
/// rule, so there is nothing here for an author to decide.
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

/// Nothing, taking up room.
///
/// Fixed with [`width`] or [`height`]; given neither, it takes whatever room
/// is going, which is how one thing is pushed to the far end of a row:
///
/// ```
/// # use omega::ui::{Row, Spacer, Text};
/// Row::new()
///     .fill()
///     .child(Text::new("Bose NC 700"))
///     .child(Spacer::new())
///     .child(Text::new("40%").dim());
/// ```
///
/// The row has to [`fill`] for there to be room to give away: a row as wide
/// as what it draws has none, and the spacer is nought pixels of nothing.
///
/// [`width`]: Spacer::width
/// [`height`]: Spacer::height
/// [`fill`]: Stack::fill
#[derive(Debug, Clone)]
pub struct Spacer {
    node: Node,
}

impl Spacer {
    /// Taking the room, until it is told a size.
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

/// Children in rows of a fixed width.
///
/// For the things a panel lays out in pairs — a label beside a figure, four
/// times over. A [`Stack`] of stacks would let each row size itself and the
/// columns would not line up.
#[derive(Debug, Clone)]
pub struct Grid {
    node: Node,
}

impl Grid {
    /// How many across. One is a column; zero is not a grid, and the shell
    /// treats it as one.
    pub fn new(columns: u32) -> Self {
        Self {
            node: Node::new("grid").number("columns", columns.max(1)),
        }
    }

    /// Space between cells, both ways.
    pub fn gap(mut self, gap: u32) -> Self {
        self.node = self.node.number("gap", gap);
        self
    }

    pub fn child(mut self, child: impl Into<Node>) -> Self {
        self.node = self.node.child(child);
        self
    }

    pub fn children<C: Into<Node>>(mut self, children: impl IntoIterator<Item = C>) -> Self {
        for child in children {
            self.node = self.node.child(child);
        }
        self
    }
}

styled!(Grid);
