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
        styled!($type, |built: $type| built.node);
    };
    ($type:ident, $convert:expr) => {
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
            pub fn muted(mut self) -> Self {
                self.node = self.node.emphasis($crate::ui::Emphasis::Muted);
                self
            }

            /// The visual importance of this node, independent of its status.
            pub fn emphasis(mut self, emphasis: $crate::ui::Emphasis) -> Self {
                self.node = self.node.emphasis(emphasis);
                self
            }
            /// Emphasize the main action or reading.
            pub fn primary(self) -> Self {
                self.emphasis($crate::ui::Emphasis::Primary)
            }
            /// A supporting action or reading.
            pub fn secondary(self) -> Self {
                self.emphasis($crate::ui::Emphasis::Secondary)
            }
            /// The meaning of feedback, independent of visual importance.
            pub fn tone(mut self, tone: $crate::ui::Tone) -> Self {
                self.node = self.node.tone(tone);
                self
            }
            /// Something needs attention.
            pub fn warning(self) -> Self {
                self.tone($crate::ui::Tone::Warning)
            }
            /// Something failed.
            pub fn error(self) -> Self {
                self.tone($crate::ui::Tone::Error)
            }
            /// Something succeeded.
            pub fn success(self) -> Self {
                self.tone($crate::ui::Tone::Success)
            }

            /// Space around it, in the shell's units.
            pub fn padding(mut self, pad: u32) -> Self {
                self.node = self.node.padding(pad);
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

            /// As wide as the room it is in, rather than as wide as what
            /// it draws: in a row the width its neighbours left over, in a
            /// column the column's own width.
            ///
            /// A node that asked for a [`width`] has one, and this does
            /// nothing. For the nodes that do not span already — a [`Stack`]
            /// in a column and a [`Separator`] do — so it marks a control or
            /// a reading, or a stack in a row taking the slack.
            ///
            /// [`Stack`]: crate::ui::Stack
            /// [`Separator`]: crate::ui::Separator
            /// [`width`]: Self::width
            pub fn fill_width(mut self) -> Self {
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
                ($convert)(built)
            }
        }
    };
}

pub(crate) use styled;
