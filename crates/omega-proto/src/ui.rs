//! The view vocabulary: the node kinds a shell draws, and the props each one
//! carries.
//!
//! `ui.proto` deliberately stops short of this. `ViewNode.type` is a string
//! and `ViewNode.props` is a `map<string, Value>`, so a tree from a newer
//! plugin degrades to the parts a shell understands instead of failing to
//! parse. What the *core* set is has to live somewhere else, and this is it —
//! the same shape [`SystemTopic`] and [`ActionKind`] already have, for the
//! same reason: a closed taxonomy the schema cannot express as an enum needs
//! one table, or it is several lists that have to agree.
//!
//! Three things read it. The SDK builds nodes and is checked against it; the
//! renderer's `Props.js` is *generated* from it, so a shell cannot read a
//! prop by a name nothing publishes; and a test asks whether every prop
//! declared here is drawn by something.
//!
//! It lives in this crate rather than the SDK because the renderer needs it
//! and must not depend on the SDK — the daemon depends on the renderer, and
//! no part of the daemon depends on what a unit is written against.
//!
//! [`SystemTopic`]: crate::SystemTopic
//! [`ActionKind`]: crate::ActionKind

use std::fmt;

/// What a prop carries, and therefore how a shell has to read it.
///
/// The distinction that matters is [`Number`] against [`Fraction`]: protobuf
/// JSON writes a 64-bit integer as a *string*, so a reader that treats the
/// two alike lays out a NaN. Naming them apart here is what lets the
/// generated reader parse one and not the other.
///
/// [`Number`]: Self::Number
/// [`Fraction`]: Self::Fraction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PropKind {
    /// `Value::string_value`.
    Text,
    /// `Value::int_value` — written as a JSON string, and parsed back.
    Number,
    /// `Value::double_value`.
    Fraction,
    /// `Value::bool_value`.
    Flag,
    /// `Value::list` of doubles: a run of readings, as a graph's points.
    Fractions,
}

impl PropKind {
    /// What a shell reads when the prop is absent, as a JavaScript literal.
    ///
    /// Absence is the common case — a prop is set only when it is not the
    /// obvious thing — so the fallback is part of the prop's meaning rather
    /// than something each call site picks for itself.
    pub const fn zero(self) -> &'static str {
        match self {
            Self::Text => "\"\"",
            Self::Number => "0",
            Self::Fraction => "0",
            Self::Flag => "false",
            Self::Fractions => "[]",
        }
    }

    /// The generated reader that decodes this kind.
    pub const fn reader(self) -> &'static str {
        match self {
            Self::Text => "readText",
            Self::Number => "readNumber",
            Self::Fraction => "readFraction",
            Self::Flag => "readFlag",
            Self::Fractions => "readFractions",
        }
    }
}

/// One property of one node kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prop {
    /// The key it takes in `ViewNode.props`.
    pub name: &'static str,
    pub kind: PropKind,
    /// What a shell reads when it is absent, as a JavaScript literal.
    pub fallback: &'static str,
}

/// Declare the node kinds and what each one carries.
///
/// A prop states a fallback only where it is not its kind's zero — an unset
/// `align` is a row, an unset `columns` is one column, and everything else
/// is empty, nought or false.
macro_rules! nodes {
    (
        shared { $( $(#[$smeta:meta])* $sprop:ident : $skind:ident $(= $sfall:literal)? ),* $(,)? }
        $(
            $(#[$kmeta:meta])*
            $Kind:ident => $kname:literal {
                $( $(#[$pmeta:meta])* $prop:ident : $pkind:ident $(= $fall:literal)? ),* $(,)?
            }
        ),* $(,)?
    ) => {
        /// The node kinds a shell draws.
        ///
        /// Closed here, open on the wire: `ViewNode.type` stays a string so a
        /// shell meeting a kind it does not know draws nothing rather than
        /// refusing the tree.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum NodeKind {
            $($(#[$kmeta])* $Kind,)*
        }

        impl NodeKind {
            /// Every kind, in declaration order.
            pub const ALL: &'static [NodeKind] = &[$(Self::$Kind,)*];

            /// The string it takes in `ViewNode.type`.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$Kind => $kname,)*
                }
            }

            /// The props only this kind carries. [`SHARED`] is the rest.
            pub const fn props(self) -> &'static [Prop] {
                match self {
                    // Named, because a fallback comes from a `const fn` and
                    // an array literal holding one is not promoted.
                    $(Self::$Kind => {
                        const PROPS: &[Prop] = &[$(Prop {
                            name: stringify!($prop),
                            kind: PropKind::$pkind,
                            fallback: nodes!(@fallback $pkind $(, $fall)?),
                        },)*];
                        PROPS
                    })*
                }
            }
        }

        /// The props any node may carry, whatever its kind.
        pub const SHARED: &[Prop] = &[$(Prop {
            name: stringify!($sprop),
            kind: PropKind::$skind,
            fallback: nodes!(@fallback $skind $(, $sfall)?),
        },)*];
    };

    (@fallback $kind:ident) => { PropKind::$kind.zero() };
    (@fallback $kind:ident, $fallback:literal) => { $fallback };
}

nodes! {
    // Every node may carry these. They are read once, for any kind, rather
    // than repeated per node — which is why they have no kind in their
    // generated reader's name.
    shared {
        /// A role in the shell's palette: `"foreground"`, `"muted"`,
        /// `"accent"`, `"urgent"`, `"background"`. Not a literal — a colour
        /// no theme chose is a widget that does not belong to the desktop
        /// it is drawn on.
        color: Text,
        bold: Flag,
        /// Drawn quieter than its neighbours.
        dim: Flag,
        /// Space around it, in the shell's units.
        pad: Number,
        /// Shown when someone hovers it.
        tooltip: Text,
        /// Drawn, but not usable.
        disabled: Flag,
        /// Not usable because something is happening to it.
        busy: Flag,
        width: Number,
        height: Number,
        /// As wide as the room it is in, rather than as wide as what it
        /// draws. In a row that is the width its neighbours left over; in a
        /// column it is the column's own width.
        ///
        /// A node that asked for a `width` has one, and this does nothing.
        /// Nor is it how a stack in a column or a rule in either comes to
        /// span: those are the shell's, from what the node *is*, so a tree
        /// carries this flag only where an author chose it.
        fill: Flag,
    }

    /// Children in a row or a column.
    Stack => "stack" {
        /// `"row"` or `"column"`.
        align: Text = "\"row\"",
        gap: Number,
    },
    /// A run of text.
    ///
    /// `size` names a role on the shell's type scale — `"caption"`,
    /// `"body"`, `"subtitle"`, `"title"`, `"heading"`, `"display"` — not a
    /// number of pixels, which a unit has no way to choose well. A string
    /// for the same reason `align` is one: the set is closed, the schema
    /// cannot say so, and a shell that meets a role it does not know falls
    /// back to body rather than refusing the tree.
    Text => "text" {
        text: Text,
        size: Text = "\"body\"",
    },
    /// A glyph from the shell's icon set.
    ///
    /// Sized on the same scale as text, so a glyph beside a `display`
    /// figure can be told to grow with it. Unset, it is whatever the shell
    /// draws icons at, which is not the same as body text.
    Icon => "icon" {
        name: Text,
        size: Text,
    },
    /// A heading over a section of a panel.
    Header => "header" { text: Text },
    /// A rule between sections.
    Separator => "separator" {},
    /// Blank space. Sized by the shared `width` and `height`.
    Spacer => "spacer" {},
    /// Children in a fixed number of columns.
    Grid => "grid" {
        columns: Number = "1",
        gap: Number,
    },
    /// Something to press.
    Button => "button" { label: Text },
    /// A reading the user can drag.
    Slider => "slider" { value: Fraction },
    /// Something on or off.
    Toggle => "toggle" { on: Flag },
    /// A line to type in.
    ///
    /// `value` is how it draws, not what it holds: the buffer belongs to the
    /// shell until the user commits it, so this is a string the unit sets and
    /// not the one being typed.
    Field => "field" {
        placeholder: Text,
        secret: Flag,
        value: Text,
    },
    /// Rows to pick from, which owns its own cursor.
    List => "list" { gap: Number },
    /// One of several options, chosen by key.
    Group => "group" { selected: Text },
    /// How full something is.
    Progress => "progress" { value: Fraction },
    /// A series, drawn against a fixed range.
    ///
    /// The range is stated rather than taken from the points, because a
    /// series scaled to its own noise shows an idle machine as one on fire.
    Graph => "graph" {
        points: Fractions,
        low: Fraction,
        high: Fraction,
    },
    /// A picture, by path or URL.
    Image => "image" { source: Text },
}

impl NodeKind {
    /// The kind this wire string names, if it is one this build knows.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.name() == name)
    }

    /// Every prop this kind can carry: its own, and the shared ones.
    pub fn every_prop(self) -> impl Iterator<Item = Prop> {
        self.props().iter().copied().chain(SHARED.iter().copied())
    }
}

impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_kind_names_one_prop_twice() {
        for kind in NodeKind::ALL {
            let mut seen: Vec<_> = kind.props().iter().map(|prop| prop.name).collect();
            let total = seen.len();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), total, "{kind} declares a prop twice");
        }
    }

    #[test]
    fn no_kind_redeclares_a_shared_prop() {
        // A kind that named `width` again would generate two readers for one
        // key, and which one a shell called would decide what it drew.
        for kind in NodeKind::ALL {
            for prop in kind.props() {
                assert!(
                    !SHARED.iter().any(|shared| shared.name == prop.name),
                    "{kind} redeclares the shared prop {:?}",
                    prop.name
                );
            }
        }
    }

    #[test]
    fn one_name_may_mean_two_things_but_never_in_one_kind() {
        // `value` is a fraction on a slider and a string on a field, which is
        // exactly why a generated reader is named for its kind as well as its
        // prop. This pins that the collision is real, so the naming scheme is
        // not quietly simplified back to something that cannot express it.
        let slider = NodeKind::Slider.props()[0];
        let field = NodeKind::Field
            .props()
            .iter()
            .find(|prop| prop.name == "value")
            .copied()
            .expect("a field carries a value");

        assert_eq!(slider.name, field.name);
        assert_ne!(slider.kind, field.kind);
    }

    #[test]
    fn every_kind_is_named_once() {
        let mut names: Vec<_> = NodeKind::ALL.iter().map(|kind| kind.name()).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total);
    }

    #[test]
    fn a_kind_round_trips_through_its_wire_name() {
        for kind in NodeKind::ALL {
            assert_eq!(NodeKind::parse(kind.name()), Some(*kind));
        }
        assert_eq!(NodeKind::parse("nothing-this-build-knows"), None);
    }
}
