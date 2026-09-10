//! What a widget drew, asked questions rather than destructured.

use omega_proto::Values;
use omega_proto::omega::{ViewNode, value};

use crate::surface::Widget;
use crate::testing::state::State;
use crate::ui::Ui;

/// What a widget drew, asked questions rather than destructured.
#[derive(Debug, Clone)]
pub struct Drawn {
    pub(super) tree: omega_proto::omega::ViewTree,
}

impl Drawn {
    /// Build a widget against some state and render it once.
    pub fn of<W: Widget>(state: &State) -> Self {
        Self::configured::<W>(state, &Values::new())
    }

    /// The same, for one instance the document configured.
    pub fn configured<W: Widget>(state: &State, settings: &Values) -> Self {
        let (context, _effects) = state.context();
        Self::of_ui(W::build(&context, settings).render())
    }

    pub fn of_ui(ui: Ui) -> Self {
        Self {
            tree: ui.into_tree(),
        }
    }

    /// Every text node, in tree order, separated by a space. What the widget
    /// reads as, which is usually the whole assertion.
    pub fn text(&self) -> String {
        self.nodes()
            .into_iter()
            .filter_map(Self::text_prop)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// One node's text, by its key.
    pub fn text_of(&self, key: &str) -> Option<String> {
        Self::text_prop(self.node(key)?)
    }

    /// One node's colour, by its key.
    pub fn color_of(&self, key: &str) -> Option<String> {
        self.prop(key, "color")
    }

    /// Any string property of any node, for what the accessors do not cover.
    pub fn prop(&self, key: &str, prop: &str) -> Option<String> {
        match self.node(key)?.props.get(prop)?.kind.as_ref()? {
            value::Kind::StringValue(text) => Some(text.clone()),
            _ => None,
        }
    }

    /// Any flag of any node — `bold`, `dim`, `fill`.
    ///
    /// `None` where the node did not set it, which is not the same as false:
    /// a widget that stopped publishing a flag and one that publishes it off
    /// are different bugs, and a test that could not tell them apart would
    /// pass for both.
    pub fn flag(&self, key: &str, flag: &str) -> Option<bool> {
        match self.node(key)?.props.get(flag)?.kind.as_ref()? {
            value::Kind::BoolValue(set) => Some(*set),
            _ => None,
        }
    }

    /// The key of the first node of a kind, in tree order.
    ///
    /// For the nodes an author never named: a panel draws one bar and one
    /// separator per section, and asking for "the progress node" is what a
    /// test means rather than the path that happens to lead to it.
    pub fn first(&self, kind: &str) -> Option<String> {
        self.nodes()
            .into_iter()
            .find(|node| node.r#type == kind)
            .map(|node| node.key.clone())
    }

    /// The node with this key, wherever it is in the tree.
    pub fn node(&self, key: &str) -> Option<&ViewNode> {
        self.nodes().into_iter().find(|node| node.key == key)
    }

    /// Every node's kind, in tree order: what the widget actually built.
    pub fn kinds(&self) -> Vec<&str> {
        self.nodes()
            .into_iter()
            .map(|node| node.r#type.as_str())
            .collect()
    }

    /// Every key in the tree, in order.
    pub fn keys(&self) -> Vec<&str> {
        self.nodes()
            .into_iter()
            .map(|node| node.key.as_str())
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tree.root.is_none()
    }

    fn text_prop(node: &ViewNode) -> Option<String> {
        match node.props.get("text")?.kind.as_ref()? {
            value::Kind::StringValue(text) => Some(text.clone()),
            _ => None,
        }
    }

    fn nodes(&self) -> Vec<&ViewNode> {
        let mut found = Vec::new();
        if let Some(root) = self.tree.root.as_ref() {
            Self::walk(root, &mut found);
        }
        found
    }

    fn walk<'a>(node: &'a ViewNode, found: &mut Vec<&'a ViewNode>) {
        found.push(node);
        for child in &node.children {
            Self::walk(child, found);
        }
    }
}
