//! Local optimistic record writes and publication receipts.
use super::PluginState;
use crate::{
    runtime::context::Context,
    wiring::{Does, Wiring},
};
use omega_proto::omega::Capability;
use std::marker::PhantomData;

/// Read and update a plugin-owned record.
/// Use in commands, reactions, or stateful behavior. Render declarations use
/// [`Watch`](super::Watch) for read-only access.
#[derive(Debug)]
pub struct Own<T: PluginState> {
    context: Context,
    owns: PhantomData<fn() -> T>,
}

impl<T: PluginState> Wiring for Own<T> {
    const CAPABILITIES: &'static [Capability] = &[Capability::StateRead, Capability::StateWrite];

    /// Declare the published keyspace for manifest validation and discovery.
    fn keyspaces() -> Vec<String> {
        vec![T::address()]
    }

    fn build(context: &Context) -> Self {
        Self {
            context: context.clone(),
            owns: PhantomData,
        }
    }
}

impl<T: PluginState> Does for Own<T> {}

impl<T: PluginState> Own<T> {
    /// The local record, initialized from the handshake snapshot or its default.
    /// Local writes are visible immediately; replication does not roll them back.
    pub fn get(&self) -> T {
        self.context.record::<T>()
    }

    /// Admit a new value for publication. Admission failure leaves local state
    /// unchanged. The receipt reports whether the daemon accepted publication.
    /// Accepted state survives this plugin restarting while the daemon runs.
    ///
    /// ```no_run
    /// # #[derive(omega::PluginState, Default, Clone)]
    /// # struct Power { on: bool }
    /// # async fn save(power: &omega::record::Own<Power>) -> Result<(), omega::Error> {
    /// power.set(&Power { on: true }).await
    /// # }
    /// ```
    pub fn set(&self, value: &T) -> crate::effect::Effect {
        crate::effect::Effect::new(
            self.context
                .update_record::<T>(|current| *current = T::read(&value.write())),
        )
    }

    /// Admit, read, change and publish under the local record lock. A rejected
    /// capacity reservation does not run `change`. An oversized result is rejected
    /// after `change`, leaving the local record unchanged. Once admitted, writes stay visible
    /// even if completion fails; rolling back could overwrite a newer write.
    ///
    /// ```no_run
    /// # #[derive(omega::PluginState, Default, Clone)]
    /// # struct Power { on: bool }
    /// # async fn toggle(power: &omega::record::Own<Power>) -> Result<(), omega::Error> {
    /// power.update(|value| value.on = !value.on).await
    /// # }
    /// ```
    pub fn update(&self, change: impl FnOnce(&mut T)) -> crate::effect::Effect {
        crate::effect::Effect::new(self.context.update_record(change))
    }
}
