use crate::{Args, Error, Input, ui::Bind};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub(crate) type Decoder<M> = Arc<dyn Fn(Args) -> Result<M, Error> + Send + Sync>;

/// Typed local bindings for one render. Captures remain in the plugin.
///
/// Each render owns its bindings; a binding from another render or instance is
/// refused. Keep node keys stable independently of these event identities.
pub struct Events<M> {
    bindings: Mutex<Bindings<M>>,
}
impl<M: Send + 'static> Events<M> {
    pub(crate) fn new() -> Self {
        Self {
            bindings: Mutex::new(Bindings {
                values: BTreeMap::new(),
                error: None,
            }),
        }
    }
    /// Decode the control's value and construct a local message.
    /// At most 4096 bindings may be created per render. Exceeding the limit
    /// rejects the complete render; no partial bindings or tree are published.
    pub fn on<I: Input>(&self, message: impl Fn(I) -> M + Send + Sync + 'static) -> Bind<I> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let mut bindings = match self.bindings.lock() {
            Ok(bindings) => bindings,
            Err(_) => return Bind::local(0),
        };
        if bindings.error.is_some() {
            return Bind::local(0);
        }
        if bindings.values.len() >= 4096 {
            bindings.error = Some(BindingError::Capacity);
            return Bind::local(0);
        }
        let id = match BindingId::allocate(&NEXT) {
            Ok(id) => id,
            Err(error) => {
                bindings.error = Some(error);
                return Bind::local(0);
            }
        };
        bindings
            .values
            .insert(id, Arc::new(move |args| I::decode(args).map(&message)));
        Bind::local(id.get())
    }
    /// Bind a message to a control that submits no value, such as a button.
    pub fn send(&self, message: M) -> Bind<()>
    where
        M: Clone + Sync,
    {
        self.on(move |()| message.clone())
    }
    pub(crate) fn finish(self) -> Result<BTreeMap<BindingId, Decoder<M>>, BindingError> {
        let bindings = self
            .bindings
            .into_inner()
            .map_err(|_| BindingError::Poisoned)?;
        match bindings.error {
            Some(error) => Err(error),
            None => Ok(bindings.values),
        }
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

struct Bindings<M> {
    values: BTreeMap<BindingId, Decoder<M>>,
    error: Option<BindingError>,
}

/// Why a render could not construct its local interaction bindings.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BindingError {
    #[error("a surface render may create at most 4096 local bindings")]
    Capacity,
    #[error("local binding identities are exhausted")]
    IdentityExhausted,
    #[error("local binding collection was interrupted")]
    Poisoned,
}

impl BindingId {
    fn allocate(next: &std::sync::atomic::AtomicU64) -> Result<Self, BindingError> {
        use std::sync::atomic::Ordering;
        let id = next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| BindingError::IdentityExhausted)?;
        std::num::NonZeroU64::new(id)
            .map(Self)
            .ok_or(BindingError::IdentityExhausted)
    }

    pub(crate) fn get(self) -> u64 {
        self.0.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhausted_identities_never_wrap_or_reuse_bindings() {
        let next = std::sync::atomic::AtomicU64::new(u64::MAX - 1);
        assert_eq!(BindingId::allocate(&next).unwrap().get(), u64::MAX - 1);
        for _ in 0..2 {
            assert_eq!(
                BindingId::allocate(&next),
                Err(BindingError::IdentityExhausted)
            );
        }
        assert_eq!(next.load(std::sync::atomic::Ordering::Relaxed), u64::MAX);
    }
}
