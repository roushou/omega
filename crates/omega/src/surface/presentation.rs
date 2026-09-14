use crate::{
    effect::{Effect, EffectError},
    runtime::context::Context,
    wiring::{Does, Wiring},
};
use omega_proto::omega::{ChangePresentation, PresentationAction, invoke};

/// Control this surface instance's presentation, without authority over its neighbors.
/// Available in behavior dependencies; using it outside an instance fails explicitly.
///
/// ```no_run
/// # async fn example(presentation: &omega::surface::Presentation) -> omega::Result<()> {
/// presentation.close().await?;
/// # Ok(()) }
/// ```
#[derive(Debug)]
pub struct Presentation {
    context: Context,
}
impl Wiring for Presentation {
    fn build(context: &Context) -> Self {
        Self {
            context: context.clone(),
        }
    }
}
impl Does for Presentation {}
impl Presentation {
    /// Dismiss this instance. Closing cancels its managed work; external operations
    /// already admitted are not undone. The model remains available on reopen.
    pub fn close(&self) -> Effect {
        self.change(PresentationAction::Close)
    }
    /// Hide this instance while keeping managed work alive.
    pub fn hide(&self) -> Effect {
        self.change(PresentationAction::Hide)
    }
    fn change(&self, action: PresentationAction) -> Effect {
        let Some(instance) = self.context.instance() else {
            return Effect::new(Err(EffectError::Refused(
                omega_proto::Refusal::precondition("presentation requires a surface instance"),
            )));
        };
        Effect::new(
            self.context
                .act(invoke::Op::ChangePresentation(ChangePresentation {
                    instance: Some(instance.wire()),
                    action: action as i32,
                })),
        )
    }
}
