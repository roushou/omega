//! One node, and the properties every node shares.

use std::collections::HashMap;

use omega_proto::omega::{Bind as WireBind, Value, ViewNode, value};

use crate::ui::bind::Bind;

/// A node in a view tree.
///
/// Built through the types in this module rather than by hand — `Text`,
/// `Row`, `Icon` — each of which is a `Node` with the properties that make
/// sense for it already set.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    kind: &'static str,
    key: Option<String>,
    props: HashMap<String, Value>,
    events: HashMap<String, WireBind>,
    children: Vec<Node>,
}

impl Node {
    pub(crate) fn new(kind: &'static str) -> Self {
        Self {
            kind,
            key: None,
            props: HashMap::new(),
            events: HashMap::new(),
            children: Vec::new(),
        }
    }

    /// Name this node, for a list whose items move.
    ///
    /// Positional keys are right until two renders disagree about what is at
    /// a position: a list that reorders, or one whose items come and go. Then
    /// the identity belongs to the item, and this is how to say so.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    // ---- shared properties ----

    /// A colour, by the part it plays. See [`Role`].
    pub fn color(self, role: Role) -> Self {
        self.text_prop("color", role.as_str())
    }

    pub fn bold(self) -> Self {
        self.flag("bold", true)
    }

    /// Draw it quieter than its neighbours.
    pub fn emphasis(self, emphasis: Emphasis) -> Self {
        self.text_prop("emphasis", emphasis.as_str())
    }

    pub fn tone(self, tone: Tone) -> Self {
        self.text_prop("tone", tone.as_str())
    }

    /// Space around the node, in the shell's units.
    pub fn padding(self, pad: u32) -> Self {
        self.number("pad", pad)
    }

    /// Text to show when someone hovers it.
    pub fn tooltip(self, tooltip: impl Into<String>) -> Self {
        self.text_prop("tooltip", tooltip)
    }

    // ---- building ----

    pub(crate) fn child(mut self, child: impl Into<Node>) -> Self {
        self.children.push(child.into());
        self
    }

    /// Bind an event to a call back into this unit.
    pub(crate) fn on<I>(mut self, event: &str, bind: impl Into<Bind<I>>) -> Self {
        self.events
            .insert(event.to_string(), bind.into().into_wire());
        self
    }

    pub(crate) fn text_prop(mut self, name: &str, text: impl Into<String>) -> Self {
        self.set(name, value::Kind::StringValue(text.into()));
        self
    }

    pub(crate) fn number(mut self, name: &str, number: u32) -> Self {
        self.set(name, value::Kind::IntValue(i64::from(number)));
        self
    }

    pub(crate) fn fraction(mut self, name: &str, fraction: f64) -> Self {
        self.set(name, value::Kind::DoubleValue(fraction));
        self
    }

    /// A run of numbers — the points of a graph.
    ///
    /// The only prop that is not one value. `Value` has carried a list all
    /// along; nothing had a use for one until something had to draw a series.
    pub(crate) fn fractions(mut self, name: &str, values: Vec<f64>) -> Self {
        self.set(
            name,
            value::Kind::List(omega_proto::omega::ListValue {
                values: values
                    .into_iter()
                    .map(|value| Value {
                        kind: Some(value::Kind::DoubleValue(value)),
                    })
                    .collect(),
            }),
        );
        self
    }

    pub(crate) fn flag(mut self, name: &str, flag: bool) -> Self {
        self.set(name, value::Kind::BoolValue(flag));
        self
    }

    fn set(&mut self, name: &str, kind: value::Kind) {
        self.props
            .insert(name.to_string(), Value { kind: Some(kind) });
    }

    /// Give every unnamed node the key its position implies.
    ///
    /// Depth-first, parent before children, so a node's key is the path to
    /// it: `0`, `0.1`, `0.1.0`. Stable across renders for as long as the
    /// shape is, which is exactly when positional identity is the truth.
    pub(crate) fn assign_keys(&mut self) {
        let parent = self.key.get_or_insert_with(|| "root".to_string());
        for (index, child) in self.children.iter_mut().enumerate() {
            child.key.get_or_insert_with(|| format!("{parent}.{index}"));
            child.assign_keys();
        }
    }

    pub(crate) fn into_wire(self) -> ViewNode {
        ViewNode {
            key: self.key.unwrap_or_default(),
            r#type: self.kind.to_string(),
            props: self.props,
            events: self.events,
            children: self.children.into_iter().map(Self::into_wire).collect(),
        }
    }
}

/// A colour, by the part it plays rather than by its value.
///
/// Closed, and deliberately with no way to name a literal. A hex here was the
/// one thing that let a plugin draw a colour the desktop's theme had never
/// heard of — which is how a bar ends up looking like nine people's taste
/// instead of one machine's. A role the shell does not know falls back to
/// whatever it inherited, so a tree from a newer plugin degrades rather than
/// drawing something arbitrary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// What text is drawn in unless it says otherwise.
    Foreground,
    /// The theme's own highlight.
    Accent,
    /// The colour behind things, for the rare node that draws on top of it.
    Background,
}

impl Role {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Foreground => "foreground",
            Self::Accent => "accent",
            Self::Background => "background",
        }
    }
}

/// How big a run of text is, as a role rather than a measurement.
///
/// A unit cannot pick a pixel size well: it does not know the theme's base
/// font, the display's scale, or what is drawn beside it. What it does know
/// is what the text is *for* — a caption under a figure, the figure itself —
/// and the shell turns that into a size on its own type scale.
///
/// [`Header`] takes none of these: being a header is already the answer.
///
/// [`Header`]: crate::ui::Header
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// Smaller than body. A label under a reading.
    Caption,
    /// What text is unless it says otherwise.
    Body,
    Subtitle,
    Title,
    Heading,
    /// The one figure a panel was opened to read.
    Display,
}

impl Size {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Caption => "caption",
            Self::Body => "body",
            Self::Subtitle => "subtitle",
            Self::Title => "title",
            Self::Heading => "heading",
            Self::Display => "display",
        }
    }
}

/// Which way a stack's children run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Row,
    Column,
}

impl Align {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Row => "row",
            Self::Column => "column",
        }
    }
}

/// Visual importance, independent of whether an operation succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    Primary,
    Secondary,
    Muted,
}
impl Emphasis {
    fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Muted => "muted",
        }
    }
}

/// The meaning of feedback. The renderer chooses its presentation from the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Warning,
    Error,
    Success,
}
impl Tone {
    fn as_str(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Success => "success",
        }
    }
}
