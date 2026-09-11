//! Content hierarchy composed from ordinary UI nodes.
use super::style::styled;
use super::{Column, Node, Size, Text};
use std::fmt::Display;

/// A titled group of related content, with consistent section spacing.
///
/// ```
/// use omega::ui::{Metric, Section};
/// use omega::Percent;
/// let panel = Section::new("Audio")
///     .child(Metric::new(Percent::whole(80)).label("Output volume"));
/// ```
#[derive(Debug, Clone)]
pub struct Section {
    node: Node,
}
impl Section {
    pub fn new(title: impl Display) -> Self {
        Self {
            node: Column::new()
                .gap(12)
                .child(Text::new(title).size(Size::Title).bold())
                .into(),
        }
    }
    pub fn child(mut self, child: impl Into<Node>) -> Self {
        self.node = self.node.child(child);
        self
    }
    pub fn children<N: Into<Node>>(mut self, children: impl IntoIterator<Item = N>) -> Self {
        for child in children {
            self.node = self.node.child(child);
        }
        self
    }
}
styled!(Section);

/// A prominent reading and an optional supporting label.
///
/// ```
/// use omega::ui::Metric;
/// let remaining = Metric::new("25:00").label("Ready to focus");
/// ```
#[derive(Debug, Clone)]
pub struct Metric {
    node: Node,
    value: String,
    label: Option<String>,
}
impl Metric {
    pub fn new(value: impl Display) -> Self {
        Self {
            node: Column::new().gap(6).into(),
            value: value.to_string(),
            label: None,
        }
    }
    pub fn label(mut self, label: impl Display) -> Self {
        self.label = Some(label.to_string());
        self
    }
}
styled!(Metric, |metric: Metric| {
    let mut node = metric
        .node
        .child(Text::new(metric.value).size(Size::Display));
    if let Some(label) = metric.label {
        node = node.child(Text::new(label).muted());
    }
    node
});
