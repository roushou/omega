//! What a plugin offers, and how the runtime builds one.
//!
//! Each surface kind is a trait an author implements and a derive that wires
//! it. The derive reads the struct's fields — this is where a plugin's
//! manifest comes from — and hands back a value built out of the runtime.

use omega_proto::omega::{Capability, Event, Value};
use omega_proto::{FromValue, IntoValue, SystemTopic, Values};

use crate::context::Context;
use crate::ui::Ui;

/// What a derive knows about a plugin type that the runtime needs.
///
/// Never implemented by hand: it is the sum of the fields, and a hand-written
/// one would be a second declaration that could disagree with them.
pub trait Wired: Sized + Send + Sync + 'static {
    /// Every topic its fields read.
    fn topics() -> Vec<SystemTopic>;

    /// Every capability its fields cost.
    fn capabilities() -> Vec<Capability>;

    /// Every plugin keyspace its fields read.
    fn keyspaces() -> Vec<String> {
        Vec::new()
    }

    /// One value, for one instance.
    fn build(context: &Context, settings: &Values) -> Self;
}

/// Something to draw.
///
/// `render` is called once the topics its fields declare have arrived, and
/// again whenever any of them changes. Identical trees are dropped by the
/// daemon, so render whenever you like — and hold nothing that *does*
/// anything, which the compiler enforces.
pub trait Widget: Wired {
    fn render(&self) -> Ui;
}

/// Something to be asked to do.
///
/// Called by `omega run`, by a keybind, by a button in this plugin's own
/// view, or by another plugin that was granted the right to. It runs with
/// this plugin's capabilities and nobody else's.
pub trait Command: Wired {
    fn call(&self, args: Args) -> Answer;
}

/// Something that happens.
///
/// A reaction runs when the daemon reports a transition — the AC was
/// unplugged, the battery went low — rather than on every state change. That
/// is why it is the half of a plugin allowed to have effects.
pub trait Reaction: Wired {
    fn fire(&self, event: &Event);
}

/// What a command was called with.
#[derive(Debug, Clone, Default)]
pub struct Args {
    values: Vec<Value>,
}

impl Args {
    pub fn new(values: Vec<Value>) -> Self {
        Self { values }
    }

    /// The argument at this position, if it is of this type.
    pub fn get<T: FromValue>(&self, index: usize) -> Option<T> {
        T::from_value(self.values.get(index)?)
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// What a command answered.
#[derive(Debug, Clone)]
pub enum Answer {
    /// It did the job and has nothing to say.
    Done,
    /// It has something to hand back.
    Value(Value),
    /// It will not, and this is why. The caller sees the message.
    Refused(String),
}

impl Answer {
    pub fn done() -> Self {
        Self::Done
    }

    pub fn value(value: impl IntoValue) -> Self {
        Self::Value(value.into_value())
    }

    pub fn refused(why: impl Into<String>) -> Self {
        Self::Refused(why.into())
    }
}

impl From<()> for Answer {
    fn from(_: ()) -> Self {
        Self::Done
    }
}

/// A command that hands back a word writes the word.
impl From<&str> for Answer {
    fn from(value: &str) -> Self {
        Self::value(value)
    }
}

impl From<String> for Answer {
    fn from(value: String) -> Self {
        Self::value(value)
    }
}
