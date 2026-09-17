//! Read replicated records from a declared plugin keyspace.
use super::PluginState;
use crate::{
    runtime::context::Context,
    wiring::{Reads, Wiring},
};
use omega_proto::omega::Capability;
use std::marker::PhantomData;

/// Read a typed record owned by this plugin or another plugin.
/// The field declares the required keyspace subscription.
#[derive(Debug)]
pub struct Watch<T: PluginState> {
    context: Context,
    watches: PhantomData<fn() -> T>,
}

impl<T: PluginState> Wiring for Watch<T> {
    const CAPABILITIES: &'static [Capability] = &[Capability::StateRead];

    /// Declare the keyspace subscription in the manifest.
    fn keyspaces() -> Vec<String> {
        vec![T::address()]
    }

    fn build(context: &Context) -> Self {
        Self {
            context: context.clone(),
            watches: PhantomData,
        }
    }
}

impl<T: PluginState> Reads for Watch<T> {}

impl<T: PluginState> Watch<T> {
    /// Return the replicated record, or `T::default()` if it has not been published.
    pub fn get(&self) -> T {
        T::read(&self.context.keyspace(&T::address()).unwrap_or_default())
    }
}
