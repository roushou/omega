//! What a widget draws.
//!
//! A tree of nodes, built by the things it is made of:
//!
//! ```
//! # use omega::ui::{Row, Text};
//! # use omega::Ui;
//! let ui: Ui = Row::new()
//!     .gap(6)
//!     .child(Text::new("80%").bold())
//!     .child(Text::new("charging").muted())
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

pub(crate) mod bind;
mod content;
mod control;
mod display;
mod layout;
mod node;
mod style;
mod text;

/// The icon set a shell draws: what [`Icon::new`] names.
pub use omega_proto::Glyph;

pub use bind::{Bind, CommandRef};
pub use content::{Metric, Section};
pub use control::{Button, Choice, Field, Form, FormInput, List, Slider, Toggle};
pub use display::{Graph, Image, Progress};
pub use layout::{Column, Grid, Row, Separator, Spacer, Stack};
pub use node::{Align, Emphasis, Node, Role, Size, Tone};
pub use text::{Header, Icon, Text};

use omega_proto::omega::ViewTree;

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
        root.assign_keys();
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

#[cfg(test)]
mod tests {
    use super::{Column, Row, Text, Ui};

    #[test]
    fn positional_keys_follow_explicit_parent_keys() {
        let ui: Ui = Column::new()
            .child(Text::new("first"))
            .child(Row::new().key("named").child(Text::new("nested")))
            .child(Row::new().child(Text::new("last")))
            .into();
        let root = ui.into_tree().root.unwrap();
        assert_eq!(root.key, "root");
        assert_eq!(root.children[0].key, "root.0");
        assert_eq!(root.children[1].key, "named");
        assert_eq!(root.children[1].children[0].key, "named.0");
        assert_eq!(root.children[2].children[0].key, "root.2.0");
    }

    #[test]
    fn explicit_empty_keys_and_named_descendants_are_preserved() {
        let ui: Ui = Row::new()
            .key("")
            .child(Text::new("positional"))
            .child(Text::new("named").key("stable"))
            .into();
        let root = ui.into_tree().root.unwrap();
        assert_eq!(root.key, "");
        assert_eq!(root.children[0].key, ".0");
        assert_eq!(root.children[1].key, "stable");
    }
}
