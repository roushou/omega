//! Shared modifiers preserve built-in builders and style component roots without wrappers.

macro_rules! modifiers {
    ($visibility:vis, $output:ty) => {
            /// Set a semantic theme color. See [`Role`](crate::ui::Role).
            $visibility fn color(self, role: $crate::ui::Role) -> $output {
                self.map_node(|node| node.color(role))
            }

            $visibility fn bold(self) -> $output {
                self.map_node(|node| node.bold())
            }

            /// Use muted foreground styling.
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
            /// Apply secondary emphasis.
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
            /// Apply error feedback styling.
            $visibility fn error(self) -> $output {
                self.tone($crate::ui::Tone::Error)
            }
            /// Apply success feedback styling.
            $visibility fn success(self) -> $output {
                self.tone($crate::ui::Tone::Success)
            }

            /// Set padding in logical pixels.
            $visibility fn padding(self, pad: u32) -> $output {
                self.map_node(|node| node.padding(pad))
            }

            /// Set hover tooltip text.
            $visibility fn tooltip(self, tooltip: impl Into<String>) -> $output {
                self.map_node(|node| node.tooltip(tooltip))
            }

            /// Disable interaction while keeping the node visible.
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

            /// Mark the node as busy and disable interaction.
            $visibility fn busy(self) -> $output {
                self.map_node(|node| node.flag("busy", true))
            }

            /// Set an explicit width in logical pixels. Defaults to content width.
            $visibility fn width(self, width: u32) -> $output {
                self.map_node(|node| node.number("width", width))
            }

            $visibility fn height(self, height: u32) -> $output {
                self.map_node(|node| node.number("height", height))
            }

            /// Fill the available width. In a row, use the remaining horizontal space;
            /// in a column, use the column width. An explicit [`width`](Self::width)
            /// takes precedence.
            $visibility fn fill_width(self) -> $output {
                self.map_node(|node| node.flag("fill", true))
            }

            /// Assign a stable identity to this node.
            ///
            /// Use keys for children that can be reordered, inserted, or removed to preserve
            /// focus and in-progress edits across renders. Keys must be unique within the view
            /// and stable across renders. Component boundaries scope their internal keys.
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
