//! State a plugin owns.
//!
//! The daemon replicates more than the machine's own topics: every plugin has
//! a keyspace of its own, `unit.<name>.<key>`, which it writes and anyone it
//! names may read. That is what turns a set of plugins into something that
//! composes — a plugin can publish a fact and another can draw it, without
//! either knowing the other exists at build time.
//!
//! The topic's address comes from the type. A state type is defined in the
//! crate that owns it, so `#[derive(UnitState)]` reads the unit's name from
//! that crate and the key from the type — and a reader writes
//! `Watch<lamp::Power>` rather than a string that a rename would quietly
//! break.
//!
//! ```no_run
//! # use omega::{Own, Watch};
//! #[derive(omega::UnitState, Default, Clone)]
//! pub struct Power {
//!     pub on: bool,
//! }
//!
//! // In the plugin that owns it: read and write.
//! # #[derive(omega::Command)]
//! struct Toggle { power: Own<Power> }
//!
//! // In any plugin that named it: read.
//! # #[derive(omega::Widget)]
//! struct Lamp { power: Watch<Power> }
//! ```

use std::marker::PhantomData;

use omega_proto::omega::{Capability, SetState, invoke};
use omega_proto::{Address, Fields};

use crate::context::Context;
use crate::wiring::{Does, Reads, Wiring};

/// A value that lives in a plugin's keyspace.
///
/// Derived. The unit is the crate that defines the type, and the key is the
/// type's own name, so neither is a string anybody types.
pub trait UnitState: Fields + Send + Sync + 'static {
    /// The plugin whose keyspace this is.
    const UNIT: &'static str;
    /// The key within it.
    const KEY: &'static str;

    /// `unit.<unit>.<key>` — how the daemon addresses it.
    fn address() -> String {
        Address::of_unit(Self::UNIT, Self::KEY).to_string()
    }
}

/// A plugin's own state: read it, and change it.
///
/// Changing state is doing something, so this belongs to a command or a
/// reaction. A widget that shows the same state holds a [`Watch`].
#[derive(Debug)]
pub struct Own<T: UnitState> {
    context: Context,
    owns: PhantomData<fn() -> T>,
}

impl<T: UnitState> Wiring for Own<T> {
    const CAPABILITIES: &'static [Capability] = &[Capability::StateRead, Capability::StateWrite];

    /// Its own keyspace needs no permission — a plugin owns it — but naming
    /// it is what lets `omega check` say what the plugin publishes, and what
    /// lets another plugin discover there is something to read.
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

impl<T: UnitState> Does for Own<T> {}

impl<T: UnitState> Own<T> {
    /// What the daemon currently holds, or this type's default if it has
    /// never been set.
    pub fn get(&self) -> T {
        read::<T>(&self.context)
    }

    /// Publish a new value. The daemon owns it from here on and replicates it
    /// like any other topic, so it outlives this process.
    pub fn set(&self, value: &T) {
        self.context.act(invoke::Op::SetState(SetState {
            topic: T::address(),
            value: Some(omega_proto::IntoValue::into_value(value.write())),
        }));
    }

    /// Read, change, publish — for a value that moves rather than is
    /// replaced.
    pub fn update(&self, change: impl FnOnce(&mut T)) {
        let mut value = self.get();
        change(&mut value);
        self.set(&value);
    }
}

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
        read::<T>(&self.context)
    }
}

/// A plugin keyspace is a bag of values, whatever the type in front of it is.
fn read<T: UnitState>(context: &Context) -> T {
    T::read(&context.keyspace(&T::address()).unwrap_or_default())
}
