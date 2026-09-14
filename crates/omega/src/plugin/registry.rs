//! What registering a surface actually stored.
//!
//! A registration has to survive losing its type: the runtime holds a list of
//! them and builds whichever one an instance calls for. Each entry keeps the
//! two things the type knew — what it declares, and how to make one.

use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc};

use omega_proto::omega::{Capability, Event, EventKind, Value};
use omega_proto::{IntoValue, SystemTopic, Values};

use crate::Input;
use crate::runtime::context::Context;
use crate::ui::View;
use crate::wiring::Wired;
use crate::{Args, Command, Reaction, Surface};

/// What every entry can do, whatever it was registered as.
type Declaration = fn(&mut BTreeSet<Capability>, &mut BTreeSet<SystemTopic>, &mut BTreeSet<String>);

/// A registered widget: which surface it draws, and how to make one per
/// instance of it.
pub(crate) struct SurfaceEntry {
    pub(crate) surface: String,
    pub(crate) unit: Option<&'static str>,
    declare: Declaration,
    required: fn() -> Vec<SystemTopic>,
    make: fn(&Context, &Values) -> Box<dyn MountedSurface>,
}

impl SurfaceEntry {
    pub(crate) fn stateful<S: crate::StatefulSurface>(surface: String) -> Self {
        Self {
            surface,
            unit: None,
            declare: |caps, topics, keys| {
                DeclarationOf::<S>::declare(caps, topics, keys);
                DeclarationOf::<S::Effects>::declare(caps, topics, keys);
            },
            required: S::required_topics,
            make: |context, settings| {
                Box::new(crate::surface::instance::Stateful::<S>::new(
                    context, settings,
                ))
            },
        }
    }

    pub(crate) fn of<W: Surface>(surface: String) -> Self {
        Self {
            surface,
            unit: None,
            declare: DeclarationOf::<W>::declare,
            required: W::required_topics,
            make: |context, settings| Box::new(W::build(context, settings)),
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

/// A widget with its type forgotten. Only `render` survives, which is all the
/// runtime ever wanted from it.
pub(crate) trait MountedSurface: Send {
    fn render(&mut self) -> View;
    fn lifecycle(&mut self, _: crate::surface::Lifecycle) -> Result<(), crate::Error> {
        Ok(())
    }
    fn mounted(&mut self) -> Result<(), crate::Error> {
        Ok(())
    }
    fn event(&mut self, _: u64, _: Args) -> Result<(), crate::Error> {
        Err(crate::Error::invalid("surface has no local messages"))
    }
    fn poll(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), crate::Error>> {
        std::task::Poll::Pending
    }
}

impl<W: Surface> MountedSurface for W {
    fn render(&mut self) -> View {
        Surface::render(self)
    }
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

/// What one registered type declares, recorded before its type is forgotten.
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
