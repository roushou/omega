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
            /// A colour, by the part it plays. See [`Role`].
            ///
            /// [`Role`]: crate::ui::Role
            pub fn color(mut self, role: $crate::ui::Role) -> Self {
                self.node = self.node.color(role);
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

            /// Draw it, but do not let it be used.
            ///
            /// A control the unit cannot serve right now — a Wi-Fi row while
            /// something else is connecting. Removing it instead would make
            /// the list jump under the cursor.
            pub fn disabled(mut self) -> Self {
                self.node = self.node.flag("disabled", true);
                self
            }

            /// Something is happening to it.
            ///
            /// Also not usable, but for a different reason, and a shell may
            /// say so differently — a spinner rather than a grey. The unit
            /// knows which of the two it means; both would be `disabled` and
            /// only one is waiting on an answer.
            pub fn busy(mut self) -> Self {
                self.node = self.node.flag("busy", true);
                self
            }

            /// How wide it is, in the shell's units. Unset, it is as wide as
            /// what it draws.
            pub fn width(mut self, width: u32) -> Self {
                self.node = self.node.number("width", width);
                self
            }

            pub fn height(mut self, height: u32) -> Self {
                self.node = self.node.number("height", height);
                self
            }

            /// As wide as the room it is in, rather than as wide as what it
            /// draws.
            ///
            /// In a row that is the width its neighbours left over, which is
            /// how a [`Spacer`] pushes what follows it to the far end. In a
            /// column it is the column's own width, which is how a bar spans
            /// a panel instead of sitting in the corner of one.
            ///
            /// A node that asked for a [`width`] has one, and this does
            /// nothing — a fixed size is a decision, and the room is only
            /// what is left after those.
            ///
            /// For the nodes that do not already: a [`Stack`] in a column
            /// spans it without being told, and a [`Separator`] always lies
            /// across the stack it is in. What is left is a control or a
            /// reading — a bar that is the panel's gauge rather than a chip
            /// beside a label — and a stack in a *row*, where taking the
            /// slack moves everything beside it and so is asked for.
            ///
            /// [`Spacer`]: crate::ui::Spacer
            /// [`Stack`]: crate::ui::Stack
            /// [`Separator`]: crate::ui::Separator
            /// [`width`]: Self::width
            pub fn fill(mut self) -> Self {
                self.node = self.node.flag("fill", true);
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
