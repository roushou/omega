//! Register plugin surfaces, commands, and reactions, then start the runtime.
//! Registrations determine the generated manifest and available endpoints.
//!
//! ```no_run
//! # use omega::platform::power::Battery;
//! # use omega::ui::Text;
//! # use omega::{View, Surface};
//! # #[derive(omega::Surface)]
//! # struct Charge { battery: Battery }
//! # impl Surface for Charge {
//! #     type Model = ();
//! #     type Message = std::convert::Infallible;
//! #     type Effects = ();
//! #     fn update(&self, _: &mut (), message: Self::Message, _: &()) -> omega::surface::Task<Self::Message> {
//! #         match message {}
//! #     }
//! #
//! #     fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View { Text::new(self.battery.charge()).into() }
//! # }
//! fn main() -> omega::Result<()> {
//!     omega::plugin!().surface(Charge).run()
//! }
//! ```

use std::collections::BTreeSet;

use omega_proto::omega::{EventKind, SurfaceKind};
use omega_proto::{Address, SystemTopic};
use omega_proto::{Manifest, Surface as Declaration};

use crate::error::Error;
use crate::plugin::registry::{CommandEntry, ReactionEntry, SurfaceEntry};
use crate::{Command, Reaction, Surface};

/// Create a plugin using the defining package's name and version.
#[macro_export]
macro_rules! plugin {
    () => {
        $crate::Plugin::named(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
    };
}

/// Plugin registration and runtime entry points.
pub struct Plugin {
    name: String,
    version: String,
    surfaces: Vec<SurfaceEntry>,
    commands: Vec<CommandEntry>,
    reactions: Vec<ReactionEntry>,
}

impl std::fmt::Debug for Plugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plugin")
            .field("name", &self.name)
            .field("surfaces", &self.surfaces.len())
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
            surfaces: Vec::new(),
            commands: Vec::new(),
            reactions: Vec::new(),
        }
    }

    /// Register the identity supplied by `derive(Surface)`.
    ///
    /// ```
    /// use omega::{View, Surface, ui::Text};
    /// #[derive(omega::Surface)]
    /// struct Indicator;
    /// impl Surface for Indicator {
    ///     type Model = ();
    ///     type Message = std::convert::Infallible;
    ///     type Effects = ();
    ///     fn update(&self, _: &mut (), message: Self::Message, _: &()) -> omega::surface::Task<Self::Message> {
    ///         match message {}
    ///     }
    ///     fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View { Text::new("Ready").into() } }
    /// let plugin = omega::plugin!().surface(Indicator);
    /// assert_eq!(plugin.manifest().unwrap().surfaces[0].id, "indicator");
    /// ```
    pub fn surface<W: Surface + crate::surface::SurfaceIdentity>(
        mut self,
        reference: impl Into<crate::surface::SurfaceRef<W>>,
    ) -> Self {
        let reference = reference.into();
        let mut entry = SurfaceEntry::of::<W>(reference.surface().into());
        entry.plugin = Some(reference.plugin());
        self.surfaces.push(entry);
        self
    }

    /// Register a surface using the plugin name as its surface ID.
    pub fn surface_default<W: Surface>(self) -> Self {
        let id = self.name.clone();
        self.surface_as::<W>(id)
    }

    /// Register a surface with an explicit surface ID.
    pub fn surface_as<W: Surface>(mut self, surface: impl Into<String>) -> Self {
        self.surfaces.push(SurfaceEntry::of::<W>(surface.into()));
        self
    }

    /// Register a typed command endpoint.
    pub fn command<C: Command>(mut self) -> Self {
        self.commands
            .push(CommandEntry::of::<C>(C::NAME.to_string()));
        self
    }

    /// Register a reaction for the specified event kind.
    pub fn on<R: Reaction>(mut self, event: EventKind) -> Self {
        self.reactions.push(ReactionEntry::of::<R>(event));
        self
    }

    /// Build and validate the plugin manifest from its registered types.
    /// Subscriptions and capabilities are aggregated from their fields.
    pub fn manifest(&self) -> Result<Manifest, Error> {
        let name = self
            .name
            .parse::<omega_proto::PluginName>()
            .map_err(|source| Error::Name(self.name.clone(), source))?;

        let mut capabilities = BTreeSet::new();
        let mut topics = BTreeSet::new();
        let mut keyspaces = BTreeSet::new();
        let mut storage = Vec::new();
        let mut surfaces = Vec::new();
        let mut commands = Vec::new();

        let mut widget_names = BTreeSet::new();
        for widget in &self.surfaces {
            if let Some(plugin) = widget.plugin
                && plugin != self.name
            {
                return Err(Error::invalid(format!(
                    "widget {} belongs to {plugin}, not {}",
                    widget.surface, self.name
                )));
            }
            if !widget_names.insert(&widget.surface) {
                return Err(Error::invalid(format!(
                    "duplicate widget: {}",
                    widget.surface
                )));
            }
            widget.declare(&mut capabilities, &mut topics, &mut keyspaces, &mut storage);
            surfaces.push(Declaration::new(
                &omega_proto::SurfaceId::try_from(widget.surface.clone())
                    .map_err(|source| Error::Name(widget.surface.clone(), source))?,
                SurfaceKind::Widget,
            ));
        }
        let mut command_names = BTreeSet::new();
        for command in &self.commands {
            if !command_names.insert(&command.name) {
                return Err(Error::invalid(format!(
                    "duplicate command: {}",
                    command.name
                )));
            }
            command.declare(&mut capabilities, &mut topics, &mut keyspaces, &mut storage);
            if command.owner != self.name {
                return Err(Error::invalid(format!(
                    "command {} belongs to {}, not {}",
                    command.name, command.owner, self.name
                )));
            }
            commands.push(command.descriptor.clone());
        }
        for reaction in &self.reactions {
            reaction.declare(&mut capabilities, &mut topics, &mut keyspaces, &mut storage);
        }

        let mut manifest = Manifest::new(&name, self.version.clone())
            .granting(capabilities)
            .exposing(surfaces)
            .serving(commands)
            .reading(
                topics
                    .into_iter()
                    .map(|topic: SystemTopic| Address::System(topic).to_string())
                    .chain(keyspaces),
            )
            .handling(self.reactions.iter().map(|reaction| reaction.event));
        manifest.storage = storage;
        manifest.command_dependencies = self
            .surfaces
            .iter()
            .flat_map(SurfaceEntry::command_dependencies)
            .chain(
                self.commands
                    .iter()
                    .flat_map(CommandEntry::command_dependencies),
            )
            .chain(
                self.reactions
                    .iter()
                    .flat_map(ReactionEntry::command_dependencies),
            )
            .collect();
        manifest
            .validate(&name)
            .map_err(|e| Error::invalid(e.to_string()))?;
        Ok(manifest)
    }

    /// Start a Tokio runtime and serve the plugin until the session ends.
    /// Use [`Self::serve`] when running inside an existing runtime.
    pub fn run(self) -> Result<(), Error> {
        // Write canonical manifest bytes for the build tool. Generation hashes
        // must be computed from these exact bytes.
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

    /// Serve the plugin using the caller's asynchronous runtime.
    pub async fn serve(self) -> Result<(), Error> {
        let manifest = self.manifest()?;
        crate::runtime::Runtime::connect(&manifest)
            .await?
            .serve(self)
            .await
    }

    pub(crate) fn surfaces(&self) -> &[SurfaceEntry] {
        &self.surfaces
    }

    pub(crate) fn commands(&self) -> &[CommandEntry] {
        &self.commands
    }

    pub(crate) fn reactions(&self) -> &[ReactionEntry] {
        &self.reactions
    }
}

pub(crate) mod registry;
mod supervision;
pub use supervision::{PluginPhase, PluginReport, Plugins};
