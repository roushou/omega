//! State a plugin owns.
//!
//! The daemon replicates more than the machine's own topics: every plugin has
//! a keyspace of its own, `unit.<name>.<key>`, which it writes and anyone it
//! names may read. One plugin publishes a fact, another draws it, and neither
//! needs the other at build time.
//!
//! The topic's address comes from the type. A state type is defined in the
//! crate that owns it, so `#[derive(UnitState)]` reads the unit's name from
//! that crate and the key from the type — and a reader writes
//! `Watch<lamp::Power>` rather than a string that a rename would quietly
//! break.
//!
//! ```no_run
//! # use omega::record::{Own, Watch};
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

use omega_proto::omega::Capability;
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
    /// The local record, initialized from the handshake snapshot or its default.
    /// Local writes are visible immediately; replication does not roll them back.
    pub fn get(&self) -> T {
        self.context.record::<T>()
    }

    /// Admit a new value for publication. Admission failure leaves local state
    /// unchanged. The receipt reports whether the daemon accepted publication.
    /// Accepted state survives this unit restarting while the daemon runs.
    ///
    /// ```no_run
    /// # #[derive(omega::UnitState, Default, Clone)]
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
    /// # #[derive(omega::UnitState, Default, Clone)]
    /// # struct Power { on: bool }
    /// # async fn toggle(power: &omega::record::Own<Power>) -> Result<(), omega::Error> {
    /// power.update(|value| value.on = !value.on).await
    /// # }
    /// ```
    pub fn update(&self, change: impl FnOnce(&mut T)) -> crate::effect::Effect {
        crate::effect::Effect::new(self.context.update_record(change))
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
