//! A plugin: what it offers, and running it.
//!
//! A plugin is not a thing you implement — it is what you registered. Each
//! registration puts a surface in the manifest and a constructor in the
//! runtime, so what the daemon grants and what the plugin can do are the same
//! list, arrived at once.
//!
//! ```no_run
//! # use omega::state::Battery;
//! # use omega::ui::Text;
//! # use omega::{Ui, Widget};
//! # #[derive(omega::Widget)]
//! # struct Charge { battery: Battery }
//! # impl Widget for Charge {
//! #     fn render(&self) -> Ui { Text::new(self.battery.charge()).into() }
//! # }
//! fn main() -> omega::Result<()> {
//!     omega::plugin!().widget::<Charge>().run()
//! }
//! ```

use std::collections::BTreeSet;

use omega_proto::omega::{EventKind, SurfaceKind};
use omega_proto::{Address, SystemTopic};
use omega_proto::{Manifest, Surface};

use crate::error::Error;
use crate::registry::{CommandEntry, ReactionEntry, WidgetEntry};
use crate::surface::{Command, Reaction, Widget};

/// Name a plugin after the crate it is.
///
/// The unit's name is its crate name — the build copies
/// `target/release/<crate>` and the daemon runs it under that name — so
/// typing it again would be a second place to get it wrong.
#[macro_export]
macro_rules! plugin {
    () => {
        $crate::Plugin::named(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
    };
}

/// The surfaces a plugin offers.
pub struct Plugin {
    name: String,
    version: String,
    widgets: Vec<WidgetEntry>,
    commands: Vec<CommandEntry>,
    reactions: Vec<ReactionEntry>,
}

impl std::fmt::Debug for Plugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plugin")
            .field("name", &self.name)
            .field("widgets", &self.widgets.len())
            .field("commands", &self.commands.len())
            .field("reactions", &self.reactions.len())
            .finish()
    }
}

impl Plugin {
    /// Prefer [`plugin!`], which fills these in from the crate.
    pub fn named(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            widgets: Vec::new(),
            commands: Vec::new(),
            reactions: Vec::new(),
        }
    }

    /// Draw something. The surface takes the plugin's own name, which is what
    /// a document refers to it by.
    pub fn widget<W: Widget>(self) -> Self {
        let id = self.name.clone();
        self.widget_as::<W>(id)
    }

    /// Draw something, under a name of its own — for a plugin with more than
    /// one surface to draw.
    pub fn widget_as<W: Widget>(mut self, surface: impl Into<String>) -> Self {
        self.widgets.push(WidgetEntry::of::<W>(surface.into()));
        self
    }

    /// Answer to being asked to do something.
    pub fn command<C: Command>(mut self, name: impl Into<String>) -> Self {
        self.commands.push(CommandEntry::of::<C>(name.into()));
        self
    }

    /// Run when something happens.
    pub fn on<R: Reaction>(mut self, event: EventKind) -> Self {
        self.reactions.push(ReactionEntry::of::<R>(event));
        self
    }

    /// What this plugin declares, as the daemon will read it.
    ///
    /// The union of every registered surface's fields. Nothing here was
    /// typed: topics come from the state a plugin holds, capabilities from
    /// the effects it holds, surfaces from what it registered.
    pub fn manifest(&self) -> Result<Manifest, Error> {
        let name = omega_proto::UnitName::parse(&self.name)
            .map_err(|source| Error::Name(self.name.clone(), source))?;

        let mut capabilities = BTreeSet::new();
        let mut topics = BTreeSet::new();
        let mut keyspaces = BTreeSet::new();
        let mut surfaces = Vec::new();

        for widget in &self.widgets {
            widget.declare(&mut capabilities, &mut topics, &mut keyspaces);
            surfaces.push(Surface::new(
                &omega_proto::SurfaceId::parse(widget.surface.clone())
                    .map_err(|source| Error::Name(widget.surface.clone(), source))?,
                SurfaceKind::Widget,
            ));
        }
        for command in &self.commands {
            command.declare(&mut capabilities, &mut topics, &mut keyspaces);
            surfaces.push(Surface::new(
                &omega_proto::SurfaceId::parse(command.name.clone())
                    .map_err(|source| Error::Name(command.name.clone(), source))?,
                SurfaceKind::Command,
            ));
        }
        for reaction in &self.reactions {
            reaction.declare(&mut capabilities, &mut topics, &mut keyspaces);
        }

        Ok(Manifest::new(&name, self.version.clone())
            .granting(capabilities)
            .exposing(surfaces)
            // The machine's topics and the plugins' own, in one list: the
            // daemon does not distinguish, and neither should a reader.
            .reading(
                topics
                    .into_iter()
                    .map(|topic: SystemTopic| Address::System(topic).to_string())
                    .chain(keyspaces),
            )
            .handling(self.reactions.iter().map(|reaction| reaction.event)))
    }

    /// Serve until the daemon goes away.
    ///
    /// Starts a runtime of its own: a plugin author should not have to know
    /// this program is asynchronous, because nothing they wrote is.
    pub fn run(self) -> Result<(), Error> {
        // `omega build` compiles a plugin and then asks it what it declares.
        // That is why there is no manifest file to keep in step with the
        // code: the code is asked.
        // The answer is the canonical encoding itself, on stdout: the build
        // stages exactly these bytes, and the hash the daemon recomputes is
        // taken over exactly these bytes. Nothing re-encodes it in between,
        // so nothing can disagree about them.
        if std::env::args().any(|argument| argument == Manifest::DESCRIBE) {
            use std::io::Write as _;
            std::io::stdout()
                .write_all(&self.manifest()?.canonical())
                .map_err(Error::Describe)?;
            return Ok(());
        }

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(Error::Runtime)?
            .block_on(self.serve())
    }

    /// Serve on a runtime the caller already has.
    pub async fn serve(self) -> Result<(), Error> {
        let manifest = self.manifest()?;
        crate::runtime::Runtime::connect(&manifest)
            .await?
            .serve(self)
            .await
    }

    pub(crate) fn widgets(&self) -> &[WidgetEntry] {
        &self.widgets
    }

    pub(crate) fn commands(&self) -> &[CommandEntry] {
        &self.commands
    }

    pub(crate) fn reactions(&self) -> &[ReactionEntry] {
        &self.reactions
    }

    /// Every topic any surface reads: what the runtime waits for before the
    /// first render.
    pub(crate) fn topics(&self) -> Vec<SystemTopic> {
        let mut topics = BTreeSet::new();
        let mut capabilities = BTreeSet::new();
        let mut keyspaces = BTreeSet::new();
        for widget in &self.widgets {
            widget.declare(&mut capabilities, &mut topics, &mut keyspaces);
        }
        // Keyspaces are not waited for: a plugin's state has a default until
        // somebody sets it, and waiting for one nobody has written yet would
        // be waiting forever.
        topics.into_iter().collect()
    }
}
