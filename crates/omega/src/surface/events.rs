use crate::{Args, Error, Input, ui::Bind};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub(crate) type Decoder<M> = Arc<dyn Fn(Args) -> Result<M, Error> + Send + Sync>;

/// Typed local bindings for one render. Captures remain in the plugin.
///
/// Each render owns its bindings; a binding from another render or instance is
/// refused. Keep node keys stable independently of these event identities.
pub struct Events<M> {
    bindings: Mutex<BTreeMap<BindingId, Decoder<M>>>,
}
impl<M: Send + 'static> Events<M> {
    pub(crate) fn new() -> Self {
        Self {
            bindings: Mutex::new(BTreeMap::new()),
        }
    }
    /// Decode the control's value and construct a local message.
    pub fn on<I: Input>(&self, message: impl Fn(I) -> M + Send + Sync + 'static) -> Bind<I> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let mut bindings = self.bindings.lock().unwrap();
        assert!(bindings.len() < 4096, "surface binding capacity exhausted");
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = BindingId::try_from(id).expect("binding identity exhausted");
        bindings.insert(id, Arc::new(move |args| I::decode(args).map(&message)));
        Bind::local(id.0.get())
    }
    /// Bind a message to a control that submits no value, such as a button.
    pub fn send(&self, message: M) -> Bind<()>
    where
        M: Clone + Sync,
    {
        self.on(move |()| message.clone())
    }
    pub(crate) fn finish(self) -> BTreeMap<BindingId, Decoder<M>> {
        self.bindings.into_inner().unwrap()
    }
}

impl<M> std::fmt::Debug for Events<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Events").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct BindingId(std::num::NonZeroU64);
impl TryFrom<u64> for BindingId {
    type Error = Error;

    fn try_from(id: u64) -> Result<Self, Self::Error> {
        std::num::NonZeroU64::new(id)
            .map(Self::from)
            .ok_or_else(|| Error::invalid("local binding identity must be nonzero"))
    }
}

impl From<std::num::NonZeroU64> for BindingId {
    fn from(value: std::num::NonZeroU64) -> Self {
        Self(value)
    }
}
