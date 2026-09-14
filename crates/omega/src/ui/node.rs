//! One node, and the properties every node shares.

use std::collections::HashMap;

use omega_proto::omega::{Bind as WireBind, Value, ViewNode, value};

use crate::ui::bind::Bind;

/// A declarative view node. Construct through UI builders such as
/// [`Text`](crate::ui::Text), [`Row`](crate::ui::Row), or [`Icon`](crate::ui::Icon).
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    kind: &'static str,
    pub(super) scope: bool,
    key: Option<String>,
    props: HashMap<String, Value>,
    events: HashMap<String, WireBind>,
    children: Vec<Node>,
}

impl Node {
    pub(crate) fn new(kind: &'static str) -> Self {
        Self {
            kind,
            scope: false,
            key: None,
            props: HashMap::new(),
            events: HashMap::new(),
            children: Vec::new(),
        }
    }

    /// Assign a stable identity to this node.
    /// Keys must be unique within the view and stable across renders.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set a semantic theme color. See [`Role`].
    pub fn color(self, role: Role) -> Self {
        self.text_prop("color", role.as_str())
    }

    pub fn bold(self) -> Self {
        self.flag("bold", true)
    }

    /// Set the node's visual emphasis.
    pub fn emphasis(self, emphasis: Emphasis) -> Self {
        self.text_prop("emphasis", emphasis.as_str())
    }

    pub fn tone(self, tone: Tone) -> Self {
        self.text_prop("tone", tone.as_str())
    }

    /// Set padding in logical pixels.
    pub fn padding(self, pad: u32) -> Self {
        self.number("pad", pad)
    }

    /// Set hover tooltip text.
    pub fn tooltip(self, tooltip: impl Into<String>) -> Self {
        self.text_prop("tooltip", tooltip)
    }

    pub(crate) fn child(mut self, child: impl Into<crate::View>) -> Self {
        if let Some(child) = child.into().root {
            self.children.push(child);
        }
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

    /// Store a list of numeric values as a node property.
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

    /// Assign hierarchical positional keys to unnamed nodes, parent before children.
    /// Explicit keys preserve item identity when siblings are reordered.
    pub(crate) fn assign_keys(&mut self) {
        self.assign_in("root".into(), None);
    }

    fn assign_in(&mut self, positional: String, scope: Option<&str>) {
        let key = match (&self.key, scope) {
            (Some(key), Some(scope)) => format!("{scope}/{}", Self::key_segment(key)),
            (Some(key), None) => key.clone(),
            (None, _) => positional,
        };
        self.key = Some(key.clone());
        let scope = if self.scope || scope.is_some() {
            Some(key.as_str())
        } else {
            scope
        };
        let selectable = matches!(self.kind, "list" | "group");
        for (index, child) in self.children.iter_mut().enumerate() {
            let value = child.key.clone();
            let positional = if scope.is_some() {
                format!("{key}/~p{index}")
            } else {
                format!("{key}.{index}")
            };
            child.assign_in(positional, scope);
            if selectable && scope.is_some() {
                // Domain values must not change when render identities are scoped.
                child.set(
                    "selection_key",
                    value::Kind::StringValue(value.unwrap_or_else(|| child.key.clone().unwrap())),
                );
            }
        }
    }

    fn key_segment(key: &str) -> String {
        key.replace('~', "~0").replace('/', "~1")
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

/// A semantic theme color. The renderer resolves each role against the active theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Default text and icon color.
    Foreground,
    /// Theme accent color.
    Accent,
    /// Theme background color.
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

/// Semantic text size resolved by the renderer.
/// [`Header`](crate::ui::Header) uses the theme's section-heading style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// Small supporting text.
    Caption,
    /// Default body text.
    Body,
    Subtitle,
    Title,
    Heading,
    /// Large display text for prominent values.
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
