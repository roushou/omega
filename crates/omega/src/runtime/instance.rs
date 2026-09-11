use omega_proto::omega::ViewTree;
use omega_proto::{SystemTopic, Values};

use crate::context::Context;
use crate::registry::{RenderedWidget, WidgetEntry};

/// Readiness and publication history belong to the configured instance's lifetime.
pub(super) struct Instance {
    pub(super) surface: String,
    pub(super) module: String,
    widget: Box<dyn RenderedWidget>,
    topics: Vec<SystemTopic>,
    sent: Option<ViewTree>,
}

impl Instance {
    pub(super) fn new(
        entry: &WidgetEntry,
        module: String,
        context: &Context,
        settings: &Values,
    ) -> Self {
        Self {
            surface: entry.surface.clone(),
            module,
            widget: entry.build(context, settings),
            topics: entry.topics(),
            sent: None,
        }
    }

    pub(super) fn view(&self, context: &Context) -> Option<ViewTree> {
        context
            .holds(&self.topics)
            .then(|| self.widget.render().into_tree())
    }

    pub(super) fn changed(&self, context: &Context) -> Option<ViewTree> {
        let view = self.view(context)?;
        (self.sent.as_ref() != Some(&view)).then_some(view)
    }

    /// Remember only trees whose socket write succeeded, including pull responses.
    pub(super) fn sent(&mut self, view: ViewTree) {
        self.sent = Some(view);
    }
}
