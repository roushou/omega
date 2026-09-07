//! How a field declares itself.
//!
//! Every field on a plugin is one of two things, and each says what it costs.
//! A [`Reads`] field is a view onto replicated state: it names the topic it
//! mirrors and needs permission to read it. A [`Does`] field is permission to
//! change something about the machine.
//!
//! This is the whole of the declaration. A plugin's manifest — the capability
//! list the daemon grants against — is the union of what its fields declare,
//! collected by `#[derive(Widget)]` and friends. There is nothing to keep in
//! sync, because using a thing and declaring it are the same act.

use omega_proto::SystemTopic;
use omega_proto::omega::Capability;

use crate::context::Context;

/// A field the runtime can build.
pub trait Wiring: Sized + Send + Sync + 'static {
    /// The state topics this field reads. The manifest subscribes to them,
    /// and the runtime waits for them before the first render.
    const TOPICS: &'static [SystemTopic] = &[];

    /// What the daemon must grant for this field to work.
    const CAPABILITIES: &'static [Capability] = &[];

    /// The plugin keyspaces it reads — `unit.<name>.<key>`, one plugin's
    /// state as another plugin sees it. Not a `SystemTopic`, so not a
    /// constant: the address comes from a type, not from the ontology.
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
    message = "a widget cannot hold `{Self}`",
    label = "this is not something to read",
    note = "a widget renders, and rendering happens on every change the machine reports — so a field that *does* something would do it on every one",
    note = "if `{Self}` acts, move it to a command or a reaction, which run when there is a reason to",
    note = "if it reads, it should be a handle like `Battery`, `Network`, or `Audio`"
)]
pub trait Reads: Wiring {}

/// A field that changes something.
///
/// Deliberately not allowed on a widget. `render` runs on every state change
/// and the daemon drops trees that did not change, so an effect there fires
/// on every percent the battery moves. Effects belong to a command or a
/// reaction, which run when something actually happened.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not something a plugin can do",
    label = "not an effect",
    note = "effects are handles like `Notify`, `Session`, `Volume`, or `Shell`"
)]
pub trait Does: Wiring {}
