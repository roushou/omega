//! What a widget draws.
//!
//! A tree of nodes, built by the things it is made of:
//!
//! ```
//! # use omega::{Row, Text, Ui};
//! let ui: Ui = Row::new()
//!     .gap(6)
//!     .child(Text::new("80%").bold())
//!     .child(Text::new("charging").dim())
//!     .into();
//! ```
//!
//! Two rules make it read like that. Anything node-shaped converts into a
//! [`Ui`], so a widget that draws one piece of text writes `Text::new(…)
//! .into()` and nothing else — there is no tree to wrap it in. And keys are
//! positional: the reconciler needs one per node, and computing them from a
//! node's place in the tree is more reliable than asking an author to invent
//! them. The exception is a list whose items move, where the identity is the
//! item's, not the position's — say so with [`Node::key`].

mod node;
mod nodes;

pub use node::{Align, Node};
pub use nodes::{Button, Column, Icon, Progress, Row, Stack, Text};

use omega_wire::omega::ViewTree;

/// A finished view: what `Widget::render` hands back.
#[derive(Debug, Clone, PartialEq)]
pub struct Ui {
    tree: ViewTree,
}

impl Ui {
    /// An empty view — a widget that has decided to draw nothing.
    ///
    /// The shell renders it as absent rather than as a gap, so a widget with
    /// nothing to say can say so.
    pub fn empty() -> Self {
        Self {
            tree: ViewTree::default(),
        }
    }

    /// The wire form. Revisions are the daemon's to assign, so this leaves
    /// the tree's at zero: units publish values.
    pub fn into_tree(self) -> ViewTree {
        self.tree
    }
}

impl<T: Into<Node>> From<T> for Ui {
    fn from(node: T) -> Self {
        let mut root = node.into();
        root.assign_keys("");
        Self {
            tree: ViewTree {
                root: Some(root.into_wire()),
                revision: 0,
            },
        }
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::empty()
    }
}
