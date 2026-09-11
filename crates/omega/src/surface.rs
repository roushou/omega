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
/// again as state patches arrive. Readiness belongs to each surface: another
/// widget's missing topic does not delay this one. Explicit absence counts as a
/// report, and unwritten records have defaults. A requested instance awaiting its
/// topics answers with an empty view and publishes once ready.
///
/// The runtime suppresses identical trees per instance before sending them.
/// Rendering holds only readings; effects are excluded by the `Reads` bound.
pub trait Widget: Wired {
    fn render(&self) -> Ui;
}

/// Something to be asked to do.
///
/// Called by `omega run`, by a keybind, by a button in this plugin's own
/// view, or by another plugin that was granted the right to. It runs with
/// this plugin's capabilities and nobody else's.
/// Commands run concurrently with socket processing. Shared mutable command
/// state must synchronize its own access; record updates already do so.
///
/// ```no_run
/// use omega::Command;
/// #[derive(omega::Command)]
/// struct Lock { session: omega::session::Session }
/// impl Command for Lock {
///     type Input = ();
///     type Output = ();
///     async fn call(&self, _: ()) -> Result<(), omega::Error> {
///         self.session.lock().await
///     }
/// }
/// ```
pub trait Command: Wired + crate::ui::bind::CommandName {
    type Input: crate::Input;
    type Output: IntoValue + Send;
    fn call(
        &self,
        args: Self::Input,
    ) -> impl std::future::Future<Output = Result<Self::Output, crate::Error>> + Send;
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

    pub(crate) fn into_values(self) -> Vec<Value> {
        self.values
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}
