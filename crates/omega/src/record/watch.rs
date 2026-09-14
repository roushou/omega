//! Read replicated records from a declared plugin keyspace.
use super::UnitState;
use crate::{
    runtime::context::Context,
    wiring::{Reads, Wiring},
};
use omega_proto::omega::Capability;
use std::marker::PhantomData;

/// Somebody's state, read only — this plugin's own, or another's.
///
/// Reading another plugin's keyspace is what its manifest declares, so
/// holding this is what asks for it.
#[derive(Debug)]
pub struct Watch<T: UnitState> {
    context: Context,
    watches: PhantomData<fn() -> T>,
}

impl<T: UnitState> Wiring for Watch<T> {
    const CAPABILITIES: &'static [Capability] = &[Capability::StateRead];

    /// Reading somebody's keyspace is what a manifest declares, so holding
    /// this is what asks for it.
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

impl<T: UnitState> Reads for Watch<T> {}

impl<T: UnitState> Watch<T> {
    /// What the daemon currently holds, or this type's default if nobody has
    /// set it.
    pub fn get(&self) -> T {
        T::read(&self.context.keyspace(&T::address()).unwrap_or_default())
    }
}
