//! Composable subtrees and reusable presentation.

use super::{Node, style::modifiers};
use omega_proto::omega::ViewTree;

/// A composable subtree, returned by widgets, components and ordinary helpers.
///
/// Empty children occupy no layout space. Keys are assigned only when the complete
/// widget is converted to its wire form.
///
/// ```
/// use omega::{View, ui::{Column, Text}};
/// let heading: View = Text::new("Memory").bold().into();
/// let panel: View = Column::new().child(heading).child(View::empty()).into();
/// assert_eq!(panel.into_tree().root.unwrap().children.len(), 1);
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct View {
    pub(super) root: Option<Node>,
}

impl View {
    /// Draw no content. Empty children add no layout slot.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Finalize keys and encode the complete widget. The daemon assigns revisions.
    ///
    /// # Panics
    /// Panics if component IDs or navigation references are invalid.
    /// Use [`Self::try_into_tree`] to handle validation errors.
    pub fn into_tree(self) -> ViewTree {
        self.try_into_tree().expect("invalid view")
    }

    /// Validate component-scoped IDs and encode a complete view.
    ///
    /// Returns an error for empty or duplicate IDs, missing references, or
    /// navigation references to controls other than lists.
    pub fn try_into_tree(self) -> Result<ViewTree, super::ViewError> {
        let root = self
            .root
            .map(|mut root| {
                root.assign_keys();
                super::navigation::References::resolve(&mut root)?;
                Ok(root.into_wire())
            })
            .transpose()?;
        Ok(ViewTree { root, revision: 0 })
    }

    pub(super) fn map_node(mut self, apply: impl FnOnce(Node) -> Node) -> Self {
        self.root = self.root.map(apply);
        self
    }

    modifiers!(pub, Self);
}

impl From<Node> for View {
    fn from(node: Node) -> Self {
        Self { root: Some(node) }
    }
}

/// Reusable presentation with explicit inputs and no runtime lifecycle.
///
/// Components receive values and typed bindings from their parent. They do not
/// register surfaces or subscriptions. Import this trait to style a component;
/// modifiers apply to its root without adding a layout wrapper.
///
/// Each instance scopes the explicit keys inside its output. Give moving instances
/// stable keys. Inside a component, keys are relative to their parent and must be
/// unique among siblings.
///
/// ```
/// use omega::{View, ui::{Component, Row, Text, Column}};
/// struct Label<'a> { text: &'a str }
/// impl Component for Label<'_> {
///     fn render(&self) -> View {
///         Row::new().child(Text::new(self.text).key("label")).into()
///     }
/// }
/// let view: View = Column::new()
///     .child(Label { text: "First" }.key("first").padding(8))
///     .child(Label { text: "Second" }.key("second"))
///     .into();
/// let root = view.into_tree().root.unwrap();
/// assert_ne!(root.children[0].children[0].key, root.children[1].children[0].key);
/// ```
///
/// Pass components directly to containers (or convert with `.into()`) to establish
/// their scope. Calling `render()` directly returns their unscoped implementation.
pub trait Component: Sized {
    /// Describe this component from its inputs; the parent owns subscriptions and effects.
    fn render(&self) -> View;

    #[doc(hidden)]
    fn map_node(self, apply: impl FnOnce(Node) -> Node) -> View {
        View::from(self).map_node(apply)
    }

    modifiers!(, View);
}

impl<C: Component> From<C> for View {
    fn from(component: C) -> Self {
        component.render().map_node(|mut root| {
            root.scope = true;
            root
        })
    }
}

impl<C: Component> Component for &C {
    fn render(&self) -> View {
        C::render(*self)
    }
}
