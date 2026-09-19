use omega_proto::omega::ViewTree;
use omega_proto::{SystemTopic, Values};

use crate::program::registration::SurfaceEntry;
use crate::runtime::context::Context;
use crate::surface::instance::MountedSurface;

/// Readiness and publication history belong to the configured instance's lifetime.
pub(super) struct Instance {
    pub(super) surface: String,
    pub(super) identity: omega_proto::instance::InstanceKey,
    widget: Box<dyn MountedSurface>,
    topics: Vec<SystemTopic>,
    sent: Option<ViewTree>,
    cached: Option<ViewTree>,
    dirty: bool,
    dependencies: Vec<String>,
}

impl Instance {
    pub(super) fn storage_changed(&mut self) {
        self.dirty = true;
    }
    pub(super) fn new(
        entry: &SurfaceEntry,
        identity: omega_proto::instance::InstanceKey,
        context: &Context,
        settings: &Values,
    ) -> Result<Self, crate::Error> {
        let context = context.for_instance(identity.clone());
        let mut widget = entry.build(&context, settings);
        widget.mounted()?;
        Ok(Self {
            surface: entry.surface.clone(),
            identity,
            widget,
            topics: entry.topics(),
            sent: None,
            cached: None,
            dirty: true,
            dependencies: entry.dependencies(),
        })
    }

    pub(super) fn view(&mut self, context: &Context) -> ViewTree {
        let pending_topics = context.missing(&self.topics);

        if !pending_topics.is_empty() {
            return ViewTree {
                pending_topics,
                readiness: omega_proto::omega::RenderReadiness::Waiting as i32,
                ..Default::default()
            };
        }
        if self.dirty {
            let tree = match self.widget.render() {
                Ok(tree) => tree,
                Err(error) => Self::failed(error),
            };
            self.cached = Some(tree);
            self.dirty = false;
        }
        self.cached
            .clone()
            .expect("clean instance has a cached render")
    }
    fn failed(error: crate::Error) -> ViewTree {
        let mut message = error.to_string();
        let mut end = message.len().min(4096);
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        ViewTree {
            readiness: omega_proto::omega::RenderReadiness::Failed as i32,
            render_error: message,
            ..Default::default()
        }
    }

    pub(super) fn changed(&mut self, context: &Context) -> Option<ViewTree> {
        let view = self.view(context);
        (self.sent.as_ref() != Some(&view)).then_some(view)
    }
    pub(super) fn invalidate(&mut self, patch: &omega_proto::omega::StatePatch) {
        if patch
            .topics
            .iter()
            .any(|topic| self.dependencies.contains(&topic.topic))
        {
            self.dirty = true;
        }
    }
    pub(super) fn lifecycle(&mut self, state: i32) -> Result<(), crate::Error> {
        use crate::surface::Lifecycle;
        use omega_proto::omega::PresentationState;
        let event = match PresentationState::try_from(state) {
            Ok(PresentationState::Visible) => Lifecycle::Presented,
            Ok(PresentationState::Hidden) => Lifecycle::Hidden,
            Ok(PresentationState::Closed) => Lifecycle::Closed,
            _ => return Err(crate::Error::invalid("unknown lifecycle state")),
        };
        self.dirty = true;
        self.widget.lifecycle(event)
    }
    pub(super) fn event(
        &mut self,
        event: &omega_proto::omega::SurfaceEvent,
    ) -> Result<(), crate::Error> {
        self.dirty = true;
        self.widget.event(
            event.binding,
            crate::Args::new(event.value.clone().into_iter().collect()),
        )
    }
    pub(super) fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), crate::Error>> {
        let result = self.widget.poll(cx);
        if result.is_ready() {
            self.dirty = true;
        }
        result
    }

    /// Remember only trees whose socket write succeeded, including pull responses.
    pub(super) fn sent(&mut self, view: ViewTree) {
        self.sent = Some(view);
    }
}
