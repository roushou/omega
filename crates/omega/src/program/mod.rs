//! Shared executable declarations and validated runtime inputs.
use crate::error::Error;
use omega_proto::omega::{HostKind, SurfaceKind};
use omega_proto::{Address, Manifest, Surface as Declaration, SystemTopic};
use registration::{CommandEntry, ReactionEntry, SurfaceEntry};
use std::collections::BTreeSet;

pub(crate) mod registration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramKind {
    Plugin,
    Commands,
}

impl ProgramKind {
    fn wire(self) -> HostKind {
        match self {
            Self::Plugin => HostKind::Plugin,
            Self::Commands => HostKind::Commands,
        }
    }
}

#[derive(Default)]
pub(crate) struct Registrations {
    pub(crate) surfaces: Vec<SurfaceEntry>,
    pub(crate) commands: Vec<CommandEntry>,
    pub(crate) reactions: Vec<ReactionEntry>,
}

pub(crate) struct Program {
    pub(crate) name: String,
    version: String,
    kind: ProgramKind,
    pub(crate) registrations: Registrations,
}

impl std::fmt::Debug for Program {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Program")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("surfaces", &self.registrations.surfaces.len())
            .field("commands", &self.registrations.commands.len())
            .field("reactions", &self.registrations.reactions.len())
            .finish()
    }
}

impl Program {
    pub(crate) fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        kind: ProgramKind,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            kind,
            registrations: Registrations::default(),
        }
    }
    pub(crate) fn prepare(self) -> Result<PreparedProgram, Error> {
        Ok(PreparedProgram {
            manifest: self.manifest()?,
            kind: self.kind,
            registrations: self.registrations,
        })
    }
    pub(crate) fn manifest(&self) -> Result<Manifest, Error> {
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
        for widget in &self.registrations.surfaces {
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
        for command in &self.registrations.commands {
            if !command_names.insert(&command.name) {
                return Err(Error::invalid(format!(
                    "duplicate command: {}",
                    command.name
                )));
            }
            command.declare(&mut capabilities, &mut topics, &mut keyspaces, &mut storage);
            commands.push(command.descriptor.clone());
        }
        for reaction in &self.registrations.reactions {
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
            .handling(
                self.registrations
                    .reactions
                    .iter()
                    .map(|reaction| reaction.event),
            );
        manifest.host_kind = self.kind.wire() as i32;
        manifest.storage = storage;
        manifest.command_dependencies = self
            .registrations
            .surfaces
            .iter()
            .flat_map(SurfaceEntry::command_dependencies)
            .chain(
                self.registrations
                    .commands
                    .iter()
                    .flat_map(CommandEntry::command_dependencies),
            )
            .chain(
                self.registrations
                    .reactions
                    .iter()
                    .flat_map(ReactionEntry::command_dependencies),
            )
            .collect();
        manifest
            .validate(&name)
            .map_err(|e| Error::invalid(e.to_string()))?;
        Ok(manifest)
    }
}

pub(crate) struct PreparedProgram {
    pub(crate) manifest: Manifest,
    pub(crate) kind: ProgramKind,
    pub(crate) registrations: Registrations,
}
impl PreparedProgram {
    pub(crate) fn run(self) -> Result<(), Error> {
        if std::env::args().any(|argument| argument == Manifest::DESCRIBE) {
            use std::io::Write;
            std::io::stdout()
                .write_all(&self.manifest.canonical())
                .map_err(Error::Describe)?;
            return Ok(());
        }
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(Error::Runtime)?
            .block_on(self.serve())
    }
    pub(crate) async fn serve(self) -> Result<(), Error> {
        crate::runtime::Runtime::connect(self).await?.serve().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Plugin, command::Construct, host::CommandHost};

    struct NeverConstruct;
    impl Construct for NeverConstruct {
        type Dependencies = (
            crate::platform::audio::Audio,
            crate::platform::audio::Volume,
        );
        fn construct(_: Self::Dependencies) -> Self {
            panic!("manifest inspection must not construct handlers")
        }
    }
    impl Command for NeverConstruct {
        type Input = ();
        type Output = ();
        const ID: &'static str = "test.inspect";
        async fn call(&self, _: ()) -> crate::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn preparation_preserves_manifest_bytes_and_retains_the_declared_registrations() {
        let plugin = Plugin::new("test", "1").command::<NeverConstruct>();
        let manifest = plugin.manifest().unwrap();
        let prepared = plugin.program.prepare().unwrap();
        assert_eq!(prepared.manifest.canonical(), manifest.canonical());
        assert_eq!(prepared.registrations.commands.len(), 1);
        let mut old_host_manifest = manifest;
        old_host_manifest.host_kind = HostKind::Commands as i32;
        let host = CommandHost::new("test", "1").command::<NeverConstruct>();
        assert_eq!(
            host.manifest().unwrap().canonical(),
            old_host_manifest.canonical()
        );
    }

    #[test]
    fn invalid_export_categories_are_rejected_during_preparation() {
        let mut program = Program::new("test", "1", ProgramKind::Commands);
        assert!(program.manifest().is_err());
        program
            .registrations
            .commands
            .push(CommandEntry::of::<NeverConstruct>(
                NeverConstruct::ID.into(),
            ));
        program
            .registrations
            .commands
            .push(CommandEntry::of::<NeverConstruct>(
                NeverConstruct::ID.into(),
            ));
        assert!(program.prepare().is_err());
    }
}
