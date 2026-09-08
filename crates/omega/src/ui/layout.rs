//! Where things go.

use crate::ui::node::{Align, Node};
use crate::ui::style::styled;

/// Children in a line.
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
