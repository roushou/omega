use omega_proto::omega::ViewTree;
use omega_proto::{SystemTopic, Values};

use crate::plugin::registry::SurfaceEntry;
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

    pub(super) fn view(&mut self, context: &Context) -> Result<Option<ViewTree>, crate::Error> {
        if !context.holds(&self.topics) {
            return Ok(None);
        }
        if self.dirty {
            self.cached = Some(self.widget.render().try_into_tree()?);
            self.dirty = false;
        }
        Ok(self.cached.clone())
    }
    pub(super) fn changed(&mut self, context: &Context) -> Result<Option<ViewTree>, crate::Error> {
        let Some(view) = self.view(context)? else {
            return Ok(None);
        };
        Ok((self.sent.as_ref() != Some(&view)).then_some(view))
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
