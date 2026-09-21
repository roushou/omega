//! Type-erased registrations retain dependency metadata and instance constructors.

use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc};

use omega_proto::omega::{Capability, Event, EventKind, SurfaceKind};
use omega_proto::{IntoValue, SystemTopic, Values};

use crate::Input;
use crate::runtime::context::Context;
use crate::surface::instance::MountedSurface;
use crate::wiring::Wired;
use crate::{Args, Command, Reaction};

/// Dependency declaration callback shared by registration kinds.
type Declaration = fn(
    &mut BTreeSet<Capability>,
    &mut BTreeSet<SystemTopic>,
    &mut BTreeSet<String>,
    &mut Vec<omega_proto::omega::StorageDescriptor>,
);

/// Surface identity and per-instance constructor.
pub(crate) struct SurfaceEntry {
    pub(crate) surface: String,
    pub(crate) plugin: Option<&'static str>,
    pub(crate) kind: SurfaceKind,
    declare: Declaration,
    commands: fn() -> Vec<omega_proto::omega::CommandDependency>,
    required: fn() -> Vec<SystemTopic>,
    make: fn(&Context, &Values) -> Box<dyn MountedSurface>,
}

impl SurfaceEntry {
    pub(crate) fn of<S: crate::Surface>(surface: String) -> Self {
        Self::with_kind::<S>(surface, SurfaceKind::Widget)
    }

    pub(crate) fn service<S: crate::Surface>(surface: String) -> Self {
        Self::with_kind::<S>(surface, SurfaceKind::Service)
    }

    fn with_kind<S: crate::Surface>(surface: String, kind: SurfaceKind) -> Self {
        Self {
            surface,
            plugin: None,
            kind,
            commands: || {
                S::commands()
                    .into_iter()
                    .chain(S::Effects::commands())
                    .collect()
            },
            declare: |caps, topics, keys, stores| {
                DeclarationOf::<S>::declare(caps, topics, keys, stores);
                DeclarationOf::<S::Effects>::declare(caps, topics, keys, stores);
            },
            required: S::required_topics,
            make: |context, settings| {
                Box::new(crate::surface::instance::Instance::<S>::new(
                    context, settings,
                ))
            },
        }
    }

    pub(crate) fn command_dependencies(&self) -> Vec<omega_proto::omega::CommandDependency> {
        (self.commands)()
    }

    pub(crate) fn declare(
        &self,
        capabilities: &mut BTreeSet<Capability>,
        topics: &mut BTreeSet<SystemTopic>,
        keyspaces: &mut BTreeSet<String>,
        storage: &mut Vec<omega_proto::omega::StorageDescriptor>,
    ) {
        (self.declare)(capabilities, topics, keyspaces, storage);
    }

    pub(crate) fn dependencies(&self) -> Vec<String> {
        let (mut caps, mut topics, mut keys) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        self.declare(&mut caps, &mut topics, &mut keys, &mut Vec::new());
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

/// A registered command.
pub(crate) struct CommandEntry {
    pub(crate) name: String,
    pub(crate) descriptor: omega_proto::omega::CommandEndpoint,
    declare: Declaration,
    commands: fn() -> Vec<omega_proto::omega::CommandDependency>,
    make: fn(&Context, &Values) -> Arc<dyn CalledCommand>,
}

impl CommandEntry {
    pub(crate) fn of<C: Command>(name: String) -> Self {
        Self {
            name,
            descriptor: crate::command::CommandRef::<C>::INSTANCE.descriptor(),
            declare: DeclarationOf::<C::Dependencies>::declare,
            commands: C::Dependencies::commands,
            make: |context, settings| {
                Arc::new(CommandFactory::<C> {
                    context: context.clone(),
                    settings: settings.clone(),
                    command: std::marker::PhantomData,
                })
            },
        }
    }

    pub(crate) fn command_dependencies(&self) -> Vec<omega_proto::omega::CommandDependency> {
        (self.commands)()
    }

    pub(crate) fn declare(
        &self,
        capabilities: &mut BTreeSet<Capability>,
        topics: &mut BTreeSet<SystemTopic>,
        keyspaces: &mut BTreeSet<String>,
        storage: &mut Vec<omega_proto::omega::StorageDescriptor>,
    ) {
        (self.declare)(capabilities, topics, keyspaces, storage);
    }

    pub(crate) fn build(&self, context: &Context, settings: &Values) -> Arc<dyn CalledCommand> {
        (self.make)(context, settings)
    }
}

pub(crate) trait CalledCommand: Send + Sync {
    fn call(
        self: Arc<Self>,
        args: Args,
    ) -> Pin<Box<dyn Future<Output = Result<omega_proto::CommandAnswer, crate::Error>> + Send>>;
}

struct CommandFactory<C> {
    context: Context,
    settings: Values,
    command: std::marker::PhantomData<fn() -> C>,
}

impl<C: Command> CalledCommand for CommandFactory<C> {
    fn call(
        self: Arc<Self>,
        args: Args,
    ) -> Pin<Box<dyn Future<Output = Result<omega_proto::CommandAnswer, crate::Error>> + Send>>
    {
        Box::pin(async move {
            use crate::command::CommandValue;
            let input = C::Input::decode(args)?;
            self.context
                .initialized(&C::Dependencies::required_topics())
                .await;
            let handler = C::construct(C::Dependencies::build(&self.context, &self.settings));
            let value = Command::call(&handler, input).await?.into_value();
            C::Output::shape()
                .accepts(&value)
                .map_err(|e| crate::Error::invalid(e.to_string()))?;
            Ok(
                if C::Output::shape().kind == omega_proto::omega::command_type::Kind::Unit as i32 {
                    omega_proto::CommandAnswer::Acknowledged
                } else {
                    omega_proto::CommandAnswer::Value(value)
                },
            )
        })
    }
}

/// A registered reaction, and the event it answers.
pub(crate) struct ReactionEntry {
    pub(crate) event: EventKind,
    declare: Declaration,
    commands: fn() -> Vec<omega_proto::omega::CommandDependency>,
    make: fn(&Context, &Values) -> Box<dyn FiredReaction>,
}

impl ReactionEntry {
    pub(crate) fn of<R: Reaction>(event: EventKind) -> Self {
        Self {
            event,
            declare: DeclarationOf::<R>::declare,
            commands: R::commands,
            make: |context, settings| Box::new(R::build(context, settings)),
        }
    }

    pub(crate) fn command_dependencies(&self) -> Vec<omega_proto::omega::CommandDependency> {
        (self.commands)()
    }

    pub(crate) fn declare(
        &self,
        capabilities: &mut BTreeSet<Capability>,
        topics: &mut BTreeSet<SystemTopic>,
        keyspaces: &mut BTreeSet<String>,
        storage: &mut Vec<omega_proto::omega::StorageDescriptor>,
    ) {
        (self.declare)(capabilities, topics, keyspaces, storage);
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
        storage: &mut Vec<omega_proto::omega::StorageDescriptor>,
    ) {
        storage.extend(T::storage());
        capabilities.extend(T::capabilities());
        topics.extend(T::topics());
        keyspaces.extend(T::keyspaces());
    }
}
