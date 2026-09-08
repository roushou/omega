//! The properties every node shares.
//!
//! A macro rather than a trait: inherent methods need no import at the call
//! site, and the typed path has to be the short one. Each returns the builder
//! rather than a [`Node`], so styling something never changes what it is —
//! two arms of a `match` that both draw text both have type [`Text`].
//!
//! [`Node`]: crate::ui::Node
//! [`Text`]: crate::ui::Text

/// Give a builder the properties every node has, each returning the builder.
macro_rules! styled {
    ($type:ident) => {
        impl $type {
            /// A colour, as the shell's theme names it (`"accent"`,
            /// `"urgent"`, `"muted"`) or as a literal (`"#ff8800"`).
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
            ///
            /// Positional keys are right until two renders disagree about
            /// what is at a position. Then the identity belongs to the item,
            /// and the shell keeps the node it already built for that key
            /// rather than rebuilding it — which is what stops a slider
            /// losing the drag in progress when its neighbours reorder.
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

pub(crate) use styled;
