//! What registering a surface actually stored.
//!
//! A registration has to survive losing its type: the runtime holds a list of
//! them and builds whichever one an instance calls for. Each entry keeps the
//! two things the type knew — what it declares, and how to make one.

use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc};

use omega_proto::omega::{Capability, Event, EventKind, Value};
use omega_proto::{IntoValue, SystemTopic, Values};

use crate::Input;
use crate::context::Context;
use crate::surface::{Args, Command, Reaction, Widget, Wired};
use crate::ui::Ui;

/// What every entry can do, whatever it was registered as.
type Declaration = fn(&mut BTreeSet<Capability>, &mut BTreeSet<SystemTopic>, &mut BTreeSet<String>);

/// A registered widget: which surface it draws, and how to make one per
/// instance of it.
pub(crate) struct WidgetEntry {
    pub(crate) surface: String,
    declare: Declaration,
    make: fn(&Context, &Values) -> Box<dyn RenderedWidget>,
}

impl WidgetEntry {
    pub(crate) fn of<W: Widget>(surface: String) -> Self {
        Self {
            surface,
            declare: declare::<W>,
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

    pub(crate) fn topics(&self) -> Vec<SystemTopic> {
        let mut topics = BTreeSet::new();
        (self.declare)(&mut BTreeSet::new(), &mut topics, &mut BTreeSet::new());
        // Keyspaces have defaults; an unwritten record must not prevent rendering.
        topics.into_iter().collect()
    }

    pub(crate) fn build(&self, context: &Context, settings: &Values) -> Box<dyn RenderedWidget> {
        (self.make)(context, settings)
    }
}

/// A widget with its type forgotten. Only `render` survives, which is all the
/// runtime ever wanted from it.
pub(crate) trait RenderedWidget: Send + Sync {
    fn render(&self) -> Ui;
}

impl<W: Widget> RenderedWidget for W {
    fn render(&self) -> Ui {
        Widget::render(self)
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
            declare: declare::<C>,
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
            declare: declare::<R>,
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
fn declare<T: Wired>(
    capabilities: &mut BTreeSet<Capability>,
    topics: &mut BTreeSet<SystemTopic>,
    keyspaces: &mut BTreeSet<String>,
) {
    capabilities.extend(T::capabilities());
    topics.extend(T::topics());
    keyspaces.extend(T::keyspaces());
}
