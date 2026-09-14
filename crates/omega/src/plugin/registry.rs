//! Type-erased registrations retain dependency metadata and instance constructors.

use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc};

use omega_proto::omega::{Capability, Event, EventKind, Value};
use omega_proto::{IntoValue, SystemTopic, Values};

use crate::Input;
use crate::runtime::context::Context;
use crate::ui::View;
use crate::wiring::Wired;
use crate::{Args, Command, Reaction};

/// Dependency declaration callback shared by registration kinds.
type Declaration = fn(&mut BTreeSet<Capability>, &mut BTreeSet<SystemTopic>, &mut BTreeSet<String>);

/// Surface identity and per-instance constructor.
pub(crate) struct SurfaceEntry {
    pub(crate) surface: String,
    pub(crate) unit: Option<&'static str>,
    declare: Declaration,
    required: fn() -> Vec<SystemTopic>,
    make: fn(&Context, &Values) -> Box<dyn MountedSurface>,
}

impl SurfaceEntry {
    pub(crate) fn of<S: crate::Surface>(surface: String) -> Self {
        Self {
            surface,
            unit: None,
            declare: |caps, topics, keys| {
                DeclarationOf::<S>::declare(caps, topics, keys);
                DeclarationOf::<S::Effects>::declare(caps, topics, keys);
            },
            required: S::required_topics,
            make: |context, settings| {
                Box::new(crate::surface::instance::Instance::<S>::new(
                    context, settings,
                ))
            },
        }
    }

    pub(crate) fn declare(
        &self,
        capabilities: &mut BTreeSet<Capability>,
        topics: &mut BTreeSet<SystemTopic>,
        keyspaces: &mut BTreeSet<String>,
    ) {
        (self.declare)(capabilities, topics, keyspaces);
    }

    pub(crate) fn dependencies(&self) -> Vec<String> {
        let (mut caps, mut topics, mut keys) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        self.declare(&mut caps, &mut topics, &mut keys);
        topics
            .into_iter()
            .map(|topic| topic.as_str().to_string())
            .chain(keys)
            .collect()
    }
    pub(crate) fn topics(&self) -> Vec<SystemTopic> {
        (self.required)()
    }

    pub(crate) fn build(&self, context: &Context, settings: &Values) -> Box<dyn MountedSurface> {
        (self.make)(context, settings)
    }
}

/// Type-erased surface rendering and lifecycle interface.
pub(crate) trait MountedSurface: Send {
    fn render(&mut self) -> View;
    fn lifecycle(&mut self, event: crate::surface::Lifecycle) -> Result<(), crate::Error>;
    fn mounted(&mut self) -> Result<(), crate::Error>;
    fn event(&mut self, binding: u64, args: Args) -> Result<(), crate::Error>;
    fn poll(
        &mut self,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), crate::Error>>;
}

/// A registered command.
pub(crate) struct CommandEntry {
    pub(crate) name: String,
    declare: Declaration,
    make: fn(&Context, &Values) -> Arc<dyn CalledCommand>,
}

impl CommandEntry {
    pub(crate) fn of<C: Command>(name: String) -> Self {
        Self {
            name,
            declare: DeclarationOf::<C>::declare,
            make: |context, settings| Arc::new(C::build(context, settings)),
        }
    }

    pub(crate) fn declare(
        &self,
        capabilities: &mut BTreeSet<Capability>,
        topics: &mut BTreeSet<SystemTopic>,
        keyspaces: &mut BTreeSet<String>,
    ) {
        (self.declare)(capabilities, topics, keyspaces);
    }

    pub(crate) fn build(&self, context: &Context, settings: &Values) -> Arc<dyn CalledCommand> {
        (self.make)(context, settings)
    }
}

pub(crate) trait CalledCommand: Send + Sync {
    fn call(
        self: Arc<Self>,
        args: Args,
    ) -> Pin<Box<dyn Future<Output = Result<Value, crate::Error>> + Send>>;
}

impl<C: Command> CalledCommand for C {
    fn call(
        self: Arc<Self>,
        args: Args,
    ) -> Pin<Box<dyn Future<Output = Result<Value, crate::Error>> + Send>> {
        Box::pin(async move {
            Command::call(&*self, C::Input::decode(args)?)
                .await
                .map(IntoValue::into_value)
        })
    }
}

/// A registered reaction, and the event it answers.
pub(crate) struct ReactionEntry {
    pub(crate) event: EventKind,
    declare: Declaration,
    make: fn(&Context, &Values) -> Box<dyn FiredReaction>,
}

impl ReactionEntry {
    pub(crate) fn of<R: Reaction>(event: EventKind) -> Self {
        Self {
            event,
            declare: DeclarationOf::<R>::declare,
            make: |context, settings| Box::new(R::build(context, settings)),
        }
    }

    pub(crate) fn declare(
        &self,
        capabilities: &mut BTreeSet<Capability>,
        topics: &mut BTreeSet<SystemTopic>,
        keyspaces: &mut BTreeSet<String>,
    ) {
        (self.declare)(capabilities, topics, keyspaces);
    }

    pub(crate) fn build(&self, context: &Context, settings: &Values) -> Box<dyn FiredReaction> {
        (self.make)(context, settings)
    }
}

pub(crate) trait FiredReaction: Send + Sync {
    fn fire(&self, event: &Event);
}

impl<R: Reaction> FiredReaction for R {
    fn fire(&self, event: &Event) {
        Reaction::fire(self, event);
    }
}

/// Collect dependency metadata before erasing the registered type.
struct DeclarationOf<T>(std::marker::PhantomData<T>);
impl<T: Wired> DeclarationOf<T> {
    fn declare(
        capabilities: &mut BTreeSet<Capability>,
        topics: &mut BTreeSet<SystemTopic>,
        keyspaces: &mut BTreeSet<String>,
    ) {
        capabilities.extend(T::capabilities());
        topics.extend(T::topics());
        keyspaces.extend(T::keyspaces());
    }
}
