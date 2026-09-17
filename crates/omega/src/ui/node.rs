//! One node, and the properties every node shares.

use std::collections::HashMap;

use omega_proto::omega::{Bind as WireBind, Value, ViewNode, value};

use crate::ui::bind::Bind;

/// A declarative view node. Construct through UI builders such as
/// [`Text`](crate::ui::Text), [`Row`](crate::ui::Row), or [`Icon`](crate::ui::Icon).
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub(super) kind: &'static str,
    pub(super) scope: bool,
    pub(super) key: Option<String>,
    pub(super) id: Option<String>,
    pub(super) navigation: Option<String>,
    pub(super) navigation_target: String,
    props: HashMap<String, Value>,
    events: HashMap<String, WireBind>,
    pub(super) children: Vec<Node>,
    shortcuts: Vec<omega_proto::omega::Shortcut>,
}

impl Node {
    pub(crate) fn new(kind: &'static str) -> Self {
        Self {
            kind,
            scope: false,
            key: None,
            id: None,
            navigation: None,
            navigation_target: String::new(),
            props: HashMap::new(),
            events: HashMap::new(),
            children: Vec::new(),
            shortcuts: Vec::new(),
        }
    }

    /// Assign a stable identity to this node.
    /// Keys must be unique within the view and stable across renders.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Address this node within its component's ID scope.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
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

    pub(crate) fn shortcuts(mut self, keys: omega_keyboard::Keymap<Bind<()>>) -> Self {
        // Replacing a map also removes its obsolete event bindings.
        for shortcut in self.shortcuts.drain(..) {
            self.events.remove(&shortcut.event);
        }
        for (index, (chord, binding)) in keys.into_bindings().enumerate() {
            let event = format!("shortcut:{index}");
            self.events.insert(event.clone(), binding.into_wire());
            self.shortcuts.push(omega_proto::omega::Shortcut {
                key: chord.key().identity(),
                modifiers: u32::from(chord.modifiers().bits()),
                release: chord.phase() == omega_keyboard::Phase::Release,
                repeat: chord.allows_repeat(),
                event,
            });
        }
        self
    }

    /// Bind an event to a call back into this plugin.
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
            shortcuts: self.shortcuts,
            navigation_target: self.navigation_target,
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

#[cfg(test)]
mod keyboard_tests {
    use crate::{
        View,
        keyboard::{Chord, Key, Keymap},
        surface::Events,
        ui::{Column, Text},
    };

    #[test]
    fn replacing_shortcuts_removes_old_bindings_and_preserves_child_scopes() {
        let events = Events::new();
        let old = Keymap::new()
            .bind(Chord::new(Key::Escape), events.send(1))
            .unwrap()
            .bind(Chord::new(Key::Enter), events.send(2))
            .unwrap();
        let new = Keymap::new()
            .bind(Chord::new(Key::Character('K')).ctrl(), events.send(3))
            .unwrap();
        let child = Keymap::new()
            .bind(Chord::new(Key::Escape), events.send(4))
            .unwrap();
        let view: View = Column::new()
            .shortcuts(old)
            .shortcuts(new)
            .child(Text::new("Child").shortcuts(child))
            .into();
        let node = view.into_tree().root.unwrap();
        assert_eq!(node.events.len(), 1);
        assert_eq!(node.shortcuts.len(), 1);
        assert_eq!(node.shortcuts[0].key, "char:k");
        assert_eq!(node.shortcuts[0].modifiers, 1);
        let binding = &node.events[&node.shortcuts[0].event];
        assert_ne!(binding.local, 0);
        assert_ne!(binding.local, node.children[0].events["shortcut:0"].local);
    }
}
