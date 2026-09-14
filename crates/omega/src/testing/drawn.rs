//! Assertions over rendered view trees.

use omega_proto::Values;
use omega_proto::omega::{ViewNode, value};

use crate::surface::Surface;
use crate::testing::state::State;
use crate::ui::View;

/// A rendered view tree with helpers for inspecting content and properties.
#[derive(Debug, Clone)]
pub struct Drawn {
    pub(super) tree: omega_proto::omega::ViewTree,
}

impl Drawn {
    pub(super) fn binding(&self, key: &str, event: &str) -> Option<&omega_proto::omega::Bind> {
        self.node(key)?.events.get(event)
    }

    /// Construct and render a surface using the production initialization and readiness rules.
    /// Use `SurfaceHarness` to retain local state and process task completions.
    pub fn of<S: Surface>(state: &State) -> crate::Result<Self> {
        Self::configured::<S>(state, &Values::new())
    }

    /// Render once with fixture readings and construction settings.
    /// Required readings that have not reported produce an empty view.
    /// Initialization that starts tasks requires a Tokio runtime.
    pub fn configured<S: Surface>(state: &State, settings: &Values) -> crate::Result<Self> {
        Ok(super::SurfaceHarness::<S>::configured(state, settings)?.draw())
    }

    /// Render a component, primitive or composed view without a daemon.
    ///
    /// ```
    /// use omega::{testing::Drawn, ui::Text};
    /// assert_eq!(Drawn::of_view(Text::new("Ready")).text(), "Ready");
    /// ```
    pub fn of_view(view: impl Into<View>) -> Self {
        Self {
            tree: view.into().into_tree(),
        }
    }

    /// Compatibility entry point; use [`Self::of_view`] for components and views.
    pub fn of_ui(ui: View) -> Self {
        Self {
            tree: ui.into_tree(),
        }
    }

    /// Return all text-node content in tree order, separated by spaces.
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

    /// Return a boolean property, or `None` if the node or property is absent.
    pub fn flag(&self, key: &str, flag: &str) -> Option<bool> {
        match self.node(key)?.props.get(flag)?.kind.as_ref()? {
            value::Kind::BoolValue(set) => Some(*set),
            _ => None,
        }
    }

    /// Return the first matching node's key in tree order.
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

    /// Return node kinds in tree order.
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

    /// The declarative tree consumed by the production renderer.
    pub fn tree(&self) -> &omega_proto::omega::ViewTree {
        &self.tree
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
