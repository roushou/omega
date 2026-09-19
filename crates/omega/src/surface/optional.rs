use crate::{
    runtime::context::Context,
    wiring::{Reads, Wiring},
};

/// Read a dependency without delaying the instance's first render.
///
/// `is_pending()` distinguishes an unreported topic from an explicit absent
/// reading. The wrapped handle retains its usual typed accessors through Deref.
#[derive(Debug)]
pub struct Optional<R: Reads> {
    reading: R,
    context: Context,
}

impl<R: Reads> Optional<R> {
    pub fn is_pending(&self) -> bool {
        !self.context.holds(R::TOPICS)
    }
}

impl<R: Reads> std::ops::Deref for Optional<R> {
    type Target = R;
    fn deref(&self) -> &R {
        &self.reading
    }
}

impl<R: Reads> Wiring for Optional<R> {
    const TOPICS: &'static [omega_proto::SystemTopic] = R::TOPICS;
    const CAPABILITIES: &'static [omega_proto::omega::Capability] = R::CAPABILITIES;
    fn required_topics() -> Vec<omega_proto::SystemTopic> {
        Vec::new()
    }
    fn keyspaces() -> Vec<String> {
        R::keyspaces()
    }
    fn build(context: &Context) -> Self {
        Self {
            reading: R::build(context),
            context: context.clone(),
        }
    }
}

impl<R: Reads> Reads for Optional<R> {}
