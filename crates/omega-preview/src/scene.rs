use omega::{
    Args,
    testing::{CapturedEffect, Drawn, State, SurfaceHarness},
};
use omega_proto::omega::PreviewInteraction;
use std::task::{Context, Poll};

pub(crate) trait Scene {
    fn draw(&mut self) -> Drawn;
    fn interact(&mut self, drawn: &Drawn, event: PreviewInteraction) -> omega::Result<()>;
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<omega::Result<()>>;
    fn effect(&mut self) -> Option<CapturedEffect>;
    fn lifecycle(&mut self, event: omega::surface::Lifecycle) -> omega::Result<()>;
}

pub(crate) struct Component<F>(pub F);
impl<F: Fn() -> omega::View> Scene for Component<F> {
    fn draw(&mut self) -> Drawn {
        Drawn::of_view((self.0)())
    }
    fn interact(&mut self, _: &Drawn, _: PreviewInteraction) -> omega::Result<()> {
        Err(omega::Error::invalid(
            "component case has no behavior; register a surface to exercise interactions",
        ))
    }
    fn poll(&mut self, _: &mut Context<'_>) -> Poll<omega::Result<()>> {
        Poll::Pending
    }
    fn effect(&mut self) -> Option<CapturedEffect> {
        None
    }
    fn lifecycle(&mut self, _: omega::surface::Lifecycle) -> omega::Result<()> {
        Ok(())
    }
}

pub(crate) struct Surface<S: omega::Surface>(SurfaceHarness<S>);
impl<S: omega::Surface> Surface<S> {
    pub(crate) fn new(state: &State) -> omega::Result<Self> {
        Self::from_harness(SurfaceHarness::new(state)?)
    }
    pub(crate) fn from_harness(mut harness: SurfaceHarness<S>) -> omega::Result<Self> {
        harness.lifecycle(omega::surface::Lifecycle::Presented)?;
        Ok(Self(harness))
    }
}
impl<S: omega::Surface> Scene for Surface<S> {
    fn draw(&mut self) -> Drawn {
        self.0.draw()
    }
    fn interact(&mut self, drawn: &Drawn, event: PreviewInteraction) -> omega::Result<()> {
        let mut stack = drawn
            .tree()
            .root
            .iter()
            .map(|node| (node, true))
            .collect::<Vec<_>>();
        let mut target = None;
        while let Some((node, enabled)) = stack.pop() {
            let enabled = enabled
                && !["disabled", "busy"].iter().any(|prop| {
                    node.props.get(*prop).is_some_and(|v| {
                        matches!(
                            v.kind,
                            Some(omega_proto::omega::value::Kind::BoolValue(true))
                        )
                    })
                });
            if node.key == event.node && target.replace(enabled).is_some() {
                return Err(omega::Error::invalid("ambiguous preview node key"));
            }
            stack.extend(node.children.iter().map(|child| (child, enabled)));
        }
        if target != Some(true) {
            return Err(omega::Error::invalid(
                "preview control is missing or disabled",
            ));
        }
        self.0.interact(
            drawn,
            &event.node,
            &event.event,
            Args::new(event.value.into_iter().collect()),
        )
    }
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<omega::Result<()>> {
        self.0.poll_task(cx)
    }
    fn effect(&mut self) -> Option<CapturedEffect> {
        self.0.take_effect()
    }
    fn lifecycle(&mut self, event: omega::surface::Lifecycle) -> omega::Result<()> {
        self.0.lifecycle(event)
    }
}
