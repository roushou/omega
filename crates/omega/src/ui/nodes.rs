//! The things a view is made of.
//!
//! Each is a thin builder over [`Node`]: it sets the node's kind and the
//! properties that only make sense for that kind. The properties every node
//! shares — colour, weight, padding — are generated onto each of them, and
//! every one of those returns the builder rather than a `Node`, so styling
//! something never changes what it is. Two arms of a `match` that both draw
//! text both have type `Text`.

use std::fmt::Display;

use crate::ui::node::{Align, Node};
use crate::units::Percent;

/// Give a builder the properties every node has, each returning the builder.
macro_rules! styled {
    ($type:ident) => {
        impl $type {
            /// A colour, as the shell's theme names it (`"accent"`,
            /// `"urgent"`) or as a literal (`"#ff8800"`).
            pub fn color(mut self, color: impl Into<String>) -> Self {
                self.node = self.node.color(color);
                self
            }

            pub fn bold(mut self) -> Self {
                self.node = self.node.bold();
                self
            }

            /// Draw it quieter than its neighbours.
            pub fn dim(mut self) -> Self {
                self.node = self.node.dim();
                self
            }

            /// Space around it, in the shell's units.
            pub fn pad(mut self, pad: u32) -> Self {
                self.node = self.node.pad(pad);
                self
            }

            /// Text to show when someone hovers it.
            pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
                self.node = self.node.tooltip(tooltip);
                self
            }

            /// Name it, for a list whose items move.
            pub fn key(mut self, key: impl Into<String>) -> Self {
                self.node = self.node.key(key);
                self
            }
        }

        impl From<$type> for Node {
            fn from(built: $type) -> Self {
                built.node
            }
        }
    };
}

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

/// A filled bar.
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

/// Something to press.
///
/// Pressing it calls one of this plugin's own commands — the same command
/// `omega run` calls, and the same one a keybind would. A button is not a way
/// to reach past the plugin that drew it.
#[derive(Debug, Clone)]
pub struct Button {
    node: Node,
}

impl Button {
    pub fn new(label: impl Display) -> Self {
        Self {
            node: Node::new("button").text_prop("label", label.to_string()),
        }
    }

    /// The command to call when it is pressed. Must be one this plugin
    /// registered, or the daemon refuses the call.
    pub fn on_press(mut self, command: impl Into<String>) -> Self {
        self.node = self.node.text_prop("command", command);
        self
    }
}

styled!(Button);

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
    pub fn children<C: Into<Node>>(mut self, children: impl IntoIterator<Item = C>) -> Self {
        for child in children {
            self.node = self.node.child(child);
        }
        self
    }
}

styled!(Stack);

/// Children left to right.
#[derive(Debug)]
pub struct Row;

impl Row {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Stack {
        Stack::along(Align::Row)
    }
}

/// Children top to bottom.
#[derive(Debug)]
pub struct Column;

impl Column {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Stack {
        Stack::along(Align::Column)
    }
}
