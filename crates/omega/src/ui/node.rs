//! One node, and the properties every node shares.

use std::collections::HashMap;

use omega_proto::omega::{Value, ViewNode, value};

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
    children: Vec<Node>,
}

impl Node {
    pub(crate) fn new(kind: &'static str) -> Self {
        Self {
            kind,
            key: None,
            props: HashMap::new(),
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

    /// A colour, as the shell's theme names it (`"accent"`, `"urgent"`) or as
    /// a literal (`"#ff8800"`).
    pub fn color(self, color: impl Into<String>) -> Self {
        self.text_prop("color", color)
    }

    pub fn bold(self) -> Self {
        self.flag("bold", true)
    }

    /// Draw it quieter than its neighbours.
    pub fn dim(self) -> Self {
        self.flag("dim", true)
    }

    /// Space around the node, in the shell's units.
    pub fn pad(self, pad: u32) -> Self {
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
    pub(crate) fn assign_keys(&mut self, prefix: &str) {
        if self.key.is_none() {
            self.key = Some(match prefix.is_empty() {
                true => "root".to_string(),
                false => prefix.to_string(),
            });
        }

        let parent = self.key.clone().unwrap_or_default();
        for (index, child) in self.children.iter_mut().enumerate() {
            child.assign_keys(&format!("{parent}.{index}"));
        }
    }

    pub(crate) fn into_wire(self) -> ViewNode {
        ViewNode {
            key: self.key.unwrap_or_default(),
            r#type: self.kind.to_string(),
            props: self.props,
            children: self.children.into_iter().map(Self::into_wire).collect(),
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
