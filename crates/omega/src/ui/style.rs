//! Shared modifiers preserve built-in builders and style component roots without wrappers.

macro_rules! modifiers {
    ($visibility:vis, $output:ty) => {
            /// A colour, by the part it plays. See [`Role`].
            ///
            /// [`Role`]: crate::ui::Role
            $visibility fn color(self, role: $crate::ui::Role) -> $output {
                self.map_node(|node| node.color(role))
            }

            $visibility fn bold(self) -> $output {
                self.map_node(|node| node.bold())
            }

            /// Draw it quieter than its neighbours.
            $visibility fn muted(self) -> $output {
                self.map_node(|node| node.emphasis($crate::ui::Emphasis::Muted))
            }

            /// The visual importance of this node, independent of its status.
            $visibility fn emphasis(self, emphasis: $crate::ui::Emphasis) -> $output {
                self.map_node(|node| node.emphasis(emphasis))
            }
            /// Emphasize the main action or reading.
            $visibility fn primary(self) -> $output {
                self.emphasis($crate::ui::Emphasis::Primary)
            }
            /// A supporting action or reading.
            $visibility fn secondary(self) -> $output {
                self.emphasis($crate::ui::Emphasis::Secondary)
            }
            /// The meaning of feedback, independent of visual importance.
            $visibility fn tone(self, tone: $crate::ui::Tone) -> $output {
                self.map_node(|node| node.tone(tone))
            }
            /// Something needs attention.
            $visibility fn warning(self) -> $output {
                self.tone($crate::ui::Tone::Warning)
            }
            /// Something failed.
            $visibility fn error(self) -> $output {
                self.tone($crate::ui::Tone::Error)
            }
            /// Something succeeded.
            $visibility fn success(self) -> $output {
                self.tone($crate::ui::Tone::Success)
            }

            /// Space around it, in the shell's units.
            $visibility fn padding(self, pad: u32) -> $output {
                self.map_node(|node| node.padding(pad))
            }

            /// Text to show when someone hovers it.
            $visibility fn tooltip(self, tooltip: impl Into<String>) -> $output {
                self.map_node(|node| node.tooltip(tooltip))
            }

            /// Draw it, but do not let it be used.
            ///
            /// A control the unit cannot serve right now — a Wi-Fi row while
            /// something else is connecting. Removing it instead would make
            /// the list jump under the cursor.
            $visibility fn disabled(self) -> $output {
                self.disabled_if(true)
            }

            /// Disable this node when the condition is true.
            ///
            /// ```
            /// use omega::ui::Button;
            /// let can_advance = false;
            /// let next = Button::new("Next").disabled_if(!can_advance);
            /// ```
            $visibility fn disabled_if(self, disabled: bool) -> $output {
                self.map_node(|node| node.flag("disabled", disabled))
            }

            /// Something is happening to it.
            ///
            /// Also not usable, but for a different reason, and a shell may
            /// say so differently — a spinner rather than a grey. The unit
            /// knows which of the two it means; both would be `disabled` and
            /// only one is waiting on an answer.
            $visibility fn busy(self) -> $output {
                self.map_node(|node| node.flag("busy", true))
            }

            /// How wide it is, in the shell's units. Unset, it is as wide as
            /// what it draws.
            $visibility fn width(self, width: u32) -> $output {
                self.map_node(|node| node.number("width", width))
            }

            $visibility fn height(self, height: u32) -> $output {
                self.map_node(|node| node.number("height", height))
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
            $visibility fn fill_width(self) -> $output {
                self.map_node(|node| node.flag("fill", true))
            }

            /// Name it, for a list whose items move.
            ///
            /// Positional keys are right until two renders disagree about
            /// what is at a position. Then the identity belongs to the item,
            /// and the shell keeps the node it already built for that key
            /// rather than rebuilding it — which is what stops a slider
            /// losing the drag in progress when its neighbours reorder.
            $visibility fn key(self, key: impl Into<String>) -> $output {
                self.map_node(|node| node.key(key))
            }
    };
}
pub(crate) use modifiers;

macro_rules! styled {
    ($type:ident) => { styled!($type, |built: $type| built.node); };
    ($type:ident, $convert:expr) => { styled!(@impl [] $type, $convert); };
    ($type:ident<$param:ident: $bound:path>) => {
        styled!(@impl [<$param: $bound>] $type<$param>, |built: $type<$param>| built.node);
    };
    (@impl [$($generics:tt)*] $type:ty, $convert:expr) => {
        impl $($generics)* $type {
            fn map_node(mut self, apply: impl FnOnce($crate::ui::Node) -> $crate::ui::Node) -> Self {
                self.node = apply(self.node);
                self
            }
            $crate::ui::style::modifiers!(pub, Self);
        }
        impl $($generics)* From<$type> for Node {
            fn from(built: $type) -> Self { ($convert)(built) }
        }
        impl $($generics)* From<$type> for $crate::View {
            fn from(built: $type) -> Self { Self::from(Node::from(built)) }
        }
    };
}
pub(crate) use styled;
