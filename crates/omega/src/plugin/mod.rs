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

use omega_proto::Manifest;
use omega_proto::omega::EventKind;

use crate::error::Error;
use crate::program::registration::{CommandEntry, ReactionEntry, SurfaceEntry};
use crate::program::{Program, ProgramKind};
use crate::{Command, Reaction, Surface};

/// Create a plugin using the defining package's name and version.
#[macro_export]
macro_rules! plugin {
    () => {
        $crate::Plugin::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
    };
}

/// Plugin registration and runtime entry points.
#[derive(Debug)]
pub struct Plugin {
    pub(crate) program: Program,
}

impl Plugin {
    /// Prefer [`plugin!`], which fills these in from the crate.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            program: Program::new(name, version, ProgramKind::Plugin),
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
        self.program.registrations.surfaces.push(entry);
        self
    }

    /// Register a surface using the plugin name as its surface ID.
    pub fn surface_default<W: Surface>(self) -> Self {
        let id = self.program.name.clone();
        self.surface_as::<W>(id)
    }

    /// Register a surface with an explicit surface ID.
    pub fn surface_as<W: Surface>(mut self, surface: impl Into<String>) -> Self {
        self.program
            .registrations
            .surfaces
            .push(SurfaceEntry::of::<W>(surface.into()));
        self
    }

    /// Register a typed command endpoint.
    pub fn command<C: Command>(mut self) -> Self {
        self.program
            .registrations
            .commands
            .push(CommandEntry::of::<C>(C::ID.to_string()));
        self
    }

    /// Register a reaction for the specified event kind.
    pub fn on<R: Reaction>(mut self, event: EventKind) -> Self {
        self.program
            .registrations
            .reactions
            .push(ReactionEntry::of::<R>(event));
        self
    }

    /// Build and validate the plugin manifest from its registered types.
    /// Subscriptions and capabilities are aggregated from their fields.
    pub fn manifest(&self) -> Result<Manifest, Error> {
        self.program.manifest()
    }

    /// Start a Tokio runtime and serve the plugin until the session ends.
    /// Use [`Self::serve`] when running inside an existing runtime.
    pub fn run(self) -> Result<(), Error> {
        self.program.prepare()?.run()
    }

    /// Serve the plugin using the caller's asynchronous runtime.
    pub async fn serve(self) -> Result<(), Error> {
        self.program.prepare()?.serve().await
    }
}

mod supervision;
pub use supervision::{PluginPhase, PluginReport, Plugins};
