//! Node kinds and property schemas shared by the SDK and renderer.
//! The SDK validates emitted properties against this table; the renderer generates
//! `Props.js` accessors from it. Wire kind names remain strings for forward compatibility.

use std::fmt;

/// Property value encoding.
/// [`Number`](Self::Number) uses protobuf int64 strings;
/// [`Fraction`](Self::Fraction) uses JSON numbers.
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
    /// JavaScript fallback value for an absent property.
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

/// Declare node kinds and their properties, including nonzero default values.
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
        /// Supported node kinds. Unknown wire kinds render no content.
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
    // Shared properties supported by every node kind.
    shared {
        /// Theme palette role: foreground, muted, accent, urgent, or background.
        color: Text,
        /// Visual importance: primary, secondary or muted.
        emphasis: Text,
        /// Feedback meaning: neutral, warning, error or success.
        tone: Text,
        bold: Flag,
        /// Drawn quieter than its neighbours.
        dim: Flag,
        /// Space around it, in the shell's plugins.
        pad: Number,
        /// Original list or choice value, independent of its scoped render key.
        selection_key: Text = "null",
        /// Shown when someone hovers it.
        tooltip: Text,
        /// Drawn, but not usable.
        disabled: Flag,
        /// Not usable because something is happening to it.
        busy: Flag,
        width: Number,
        height: Number,
        /// Fill available width unless an explicit width is set.
        fill: Flag,
    }

    /// Children in a row or a column.
    Stack => "stack" {
        /// `"row"` or `"column"`.
        align: Text = "\"row\"",
        gap: Number,
    },
    /// Text with a semantic size role. Unknown size names fall back to body size.
    Text => "text" {
        text: Text,
        size: Text = "\"body\"",
    },
    /// Named glyph with an optional semantic size. Unset size uses the theme icon size.
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
    Button => "button" { label: Text, icon: Text, flat: Flag },
    /// A reading the user can drag.
    Slider => "slider" { value: Fraction },
    /// Something on or off.
    Toggle => "toggle" { on: Flag },
    /// Text input with optional model value and controlled-edit metadata.
    Form => "form" { label: Text },
    Field => "field" {
        size: Text,
        navigation: Text, controlled: Flag, edit_revision: Number, reset_revision: Number, autofocus: Flag,
        label: Text,
        help: Text,
        name: Text,
        placeholder: Text,
        secret: Flag,
        value: Text,
    },
    /// Rows to pick from, which owns its own cursor.
    List => "list" { gap: Number, selected: Text = "null" },
    /// One of several options, chosen by key.
    Group => "group" { selected: Text = "null" },
    /// How full something is.
    Progress => "progress" { value: Fraction },
    /// Numeric series with optional explicit scale bounds.
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
    pub fn from_name(name: &str) -> Option<Self> {
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
        // Property types depend on node kind: slider value is numeric, field value is text.
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
            assert_eq!(NodeKind::from_name(kind.name()), Some(*kind));
        }
        assert_eq!(NodeKind::from_name("nothing-this-build-knows"), None);
    }
}

/// Invalid startup readiness accompanying a published view.
#[derive(Debug, thiserror::Error)]
#[error("invalid view readiness: {0}")]
pub struct ReadinessError(&'static str);

impl crate::omega::ViewTree {
    /// Validate render metadata. Waiting trees have no root and name only system
    /// topics. Failed trees have no root or pending topics and carry a nonblank
    /// diagnostic of at most 4096 bytes. Other states carry no diagnostic.
    pub fn validate_readiness(&self) -> Result<(), ReadinessError> {
        use crate::omega::RenderReadiness;

        if self.readiness != RenderReadiness::Failed as i32 && !self.render_error.is_empty() {
            return Err(ReadinessError("render errors require failed readiness"));
        }
        match RenderReadiness::try_from(self.readiness) {
            Ok(RenderReadiness::Failed) => {
                if self.root.is_some()
                    || !self.pending_topics.is_empty()
                    || self.render_error.trim().is_empty()
                    || self.render_error.len() > 4096
                {
                    return Err(ReadinessError(
                        "failed renders require a diagnostic of at most 4096 bytes, no root, and no pending topics",
                    ));
                }
            }
            Ok(RenderReadiness::Waiting) => {
                if self.root.is_some() || self.pending_topics.is_empty() {
                    return Err(ReadinessError(
                        "waiting requires pending topics and no root",
                    ));
                }

                let mut topics = std::collections::BTreeSet::new();
                for topic in &self.pending_topics {
                    let topic = topic
                        .parse::<crate::SystemTopic>()
                        .map_err(|_| ReadinessError("unknown required system topic"))?;
                    if !topics.insert(topic) {
                        return Err(ReadinessError("duplicate required system topic"));
                    }
                }
            }
            Ok(RenderReadiness::Ready | RenderReadiness::Unspecified) => {
                if !self.pending_topics.is_empty() {
                    return Err(ReadinessError("pending topics require waiting readiness"));
                }
            }
            Err(_) => return Err(ReadinessError("unknown render readiness")),
        }

        Ok(())
    }

    /// A legacy nonempty tree proves rendering occurred. A legacy empty tree
    /// cannot distinguish a startup gate from a deliberately empty render.
    pub fn render_readiness(&self) -> crate::omega::RenderReadiness {
        use crate::omega::RenderReadiness;

        match RenderReadiness::try_from(self.readiness) {
            Ok(RenderReadiness::Unspecified) if self.root.is_some() => RenderReadiness::Ready,
            Ok(readiness) => readiness,
            Err(_) => RenderReadiness::Unspecified,
        }
    }
}

#[cfg(test)]
mod readiness_tests {
    use crate::omega::{RenderReadiness, ViewNode, ViewTree};

    #[test]
    fn waiting_metadata_is_validated_and_legacy_empty_trees_remain_unknown() {
        let mut tree = ViewTree {
            pending_topics: vec!["network".into()],
            readiness: RenderReadiness::Waiting as i32,
            ..Default::default()
        };
        assert!(tree.validate_readiness().is_ok());
        tree.root = Some(ViewNode::default());
        assert!(tree.validate_readiness().is_err());
        tree.root = None;
        tree.pending_topics.push("network".into());
        assert!(tree.validate_readiness().is_err());
        tree.pending_topics = vec!["not-a-topic".into()];
        assert!(tree.validate_readiness().is_err());
        tree.pending_topics.clear();
        assert!(tree.validate_readiness().is_err());
        tree.readiness = RenderReadiness::Ready as i32;
        assert!(tree.validate_readiness().is_ok());
        assert_eq!(tree.render_readiness(), RenderReadiness::Ready);
        tree.readiness = 0;
        assert_eq!(tree.render_readiness(), RenderReadiness::Unspecified);
        tree.root = Some(ViewNode::default());
        assert_eq!(tree.render_readiness(), RenderReadiness::Ready);
    }
}

#[cfg(test)]
mod failed_readiness_tests {
    use crate::omega::{RenderReadiness, ViewNode, ViewTree};

    #[test]
    fn render_failures_are_bounded_and_cannot_retain_interactive_content() {
        let failed = ViewTree {
            readiness: RenderReadiness::Failed as i32,
            render_error: "binding capacity exceeded".into(),
            ..Default::default()
        };
        assert!(failed.validate_readiness().is_ok());
        for invalid in [
            ViewTree {
                root: Some(ViewNode::default()),
                ..failed.clone()
            },
            ViewTree {
                pending_topics: vec!["battery".into()],
                ..failed.clone()
            },
            ViewTree {
                render_error: String::new(),
                ..failed.clone()
            },
            ViewTree {
                render_error: "x".repeat(4097),
                ..failed.clone()
            },
            ViewTree {
                readiness: RenderReadiness::Ready as i32,
                ..failed
            },
        ] {
            assert!(invalid.validate_readiness().is_err());
        }
    }
}
