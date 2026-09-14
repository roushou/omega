//! How a field declares itself.
//!
//! Every field on a plugin is one of two things, and each says what it costs.
//! A [`Reads`] field is a view onto replicated state: it names the topic it
//! mirrors and needs permission to read it. A [`Does`] field is permission to
//! change something about the machine.
//!
//! This is the whole of the declaration. A plugin's manifest — the capability
//! list the daemon grants against — is the union of what its fields declare,
//! collected by `#[derive(Surface)]` and friends. There is nothing to keep in
//! sync, because using a thing and declaring it are the same act.

use omega_proto::omega::Capability;
use omega_proto::{SystemTopic, Values};

use crate::runtime::context::Context;

/// A field the runtime can build.
pub trait Wiring: Sized + Send + Sync + 'static {
    /// The state topics this field reads and the manifest subscribes to.
    const TOPICS: &'static [SystemTopic] = &[];

    /// What the daemon must grant for this field to work.
    const CAPABILITIES: &'static [Capability] = &[];

    /// Topics that must be reported before the first render.
    fn required_topics() -> Vec<SystemTopic> {
        Self::TOPICS.to_vec()
    }

    /// Plugin record addresses this declaration subscribes to.
    fn keyspaces() -> Vec<String> {
        Vec::new()
    }

    fn build(context: &Context) -> Self;
}

/// A field that reads replicated state.
///
/// Safe to hold on anything, including a widget: reading is what rendering
/// is made of.
#[diagnostic::on_unimplemented(
    message = "a render declaration cannot hold `{Self}`",
    label = "this is not something to read",
    note = "render declarations accept reading handles only",
    note = "if `{Self}` acts, move it to a command, reaction, or the stateful surface’s Effects declaration",
    note = "if it reads, it should be a handle like `Battery`, `Network`, or `Audio`"
)]
pub trait Reads: Wiring {}

/// A field that changes something.
///
/// Effects belong to commands, reactions, or stateful behavior dependencies.
/// Render declarations require `Reads` and cannot hold them.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not something a plugin can do",
    label = "not an effect",
    note = "effects are handles like `Notify`, `Session`, `Volume`, or `Shell`"
)]
pub trait Does: Wiring {}

mod effect;
mod reading;
pub(crate) use effect::does;
pub(crate) use reading::reading;

/// What a derive knows about a plugin type that the runtime needs.
///
/// Never implemented by hand: it is the sum of the fields, and a hand-written
/// one would be a second declaration that could disagree with them.
pub trait Wired: Sized + Send + Sync + 'static {
    /// Every topic its fields read.
    fn topics() -> Vec<SystemTopic>;

    /// Every capability its fields cost.
    fn capabilities() -> Vec<Capability>;

    fn required_topics() -> Vec<SystemTopic> {
        Self::topics()
    }

    /// Every plugin keyspace its fields read.
    fn keyspaces() -> Vec<String> {
        Vec::new()
    }

    /// One value, for one instance.
    fn build(context: &Context, settings: &Values) -> Self;
}
