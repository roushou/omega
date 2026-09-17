mod source;
pub use source::{LinkError, SourceTree};

use crate::scaffold::Scaffold;
use crate::ui::Paint;
use crate::workspace::{ConfigWorkspace, FileEdit, FileEdits};
use anyhow::Result;
use omega_host::Layout;
use omega_host::cargo::{Config, Dependencies, Manifest};

/// Local Cargo overrides, independent of CLI arguments and reporting.
#[derive(Debug)]
pub(crate) struct CheckoutLink<'a> {
    workspace: &'a ConfigWorkspace,
}

#[derive(Debug)]
pub(crate) struct PreparedLink<'a> {
    _workspace: &'a ConfigWorkspace,
    edits: FileEdits,
}

impl<'a> CheckoutLink<'a> {
    /// Report whether missing local overrides could explain dependency-resolution failure.
    pub(crate) fn unlinked(layout: &Layout) -> Option<String> {
        let linked = layout
            .file::<Config>(())
            .read_or_default()
            .ok()
            .and_then(|config| {
                config
                    .patches(Config::REGISTRY)
                    .ok()
                    .map(|patched| !patched.is_empty())
            })
            .unwrap_or(false);

        if linked {
            None
        } else {
            Some(format!(
                "this config is not linked to a checkout of omega, and asks for crates that may not be published yet — point it at one with {}",
                Paint::command("omega link <path>")
            ))
        }
    }
    pub(crate) fn new(workspace: &'a ConfigWorkspace) -> Self {
        Self { workspace }
    }

    pub(crate) fn prepare(&self, source: Option<&SourceTree>) -> Result<PreparedLink<'a>> {
        let layout = self.workspace.layout();
        let mut config = FileEdit::read(layout.cargo_config())?;
        let mut editor = config.source().parse::<Config>()?;
        let root_source = FileEdit::read(layout.workspace_manifest())?;
        let root_editor = root_source.source().parse::<Manifest>()?;
        let dependencies = root_editor
            .workspace()?
            .map(|workspace| workspace.dependencies())
            .transpose()?
            .unwrap_or_default();
        let preview = Scaffold::PREVIEW_DEPENDENCIES
            .iter()
            .any(|spec| dependencies.contains(spec.name));
        let patch = match source {
            Some(source) => source.patch(preview)?,
            None => Dependencies::new(),
        };
        let specs = Scaffold::omega_crates()
            .chain(Scaffold::PREVIEW_DEPENDENCIES.iter().filter(|_| preview))
            .collect::<Vec<_>>();
        editor.update_patches(
            Config::REGISTRY,
            &Scaffold::omega_crates()
                .chain(Scaffold::PREVIEW_DEPENDENCIES)
                .map(|s| s.package())
                .collect::<Vec<_>>(),
            &patch,
        )?;
        config.replace(editor.to_string());
        let mut edits = Vec::new();
        if let Some(source) = source {
            let mut root = FileEdit::read(layout.workspace_manifest())?;
            let mut editor = root.source().parse::<Manifest>()?;
            editor.require_versions(
                &specs.iter().map(|s| s.name).collect::<Vec<_>>(),
                &source.version()?,
            )?;
            root.replace(editor.to_string());
            edits.push(root);
        }
        // Do not create an empty Cargo config when there was no local override.
        if config.exists() || !config.source().is_empty() {
            edits.push(config);
        }
        Ok(PreparedLink {
            _workspace: self.workspace,
            edits: FileEdits::new(edits),
        })
    }
}

impl PreparedLink<'_> {
    pub(crate) fn replacements(&self) -> Result<Vec<omega_host::recovery::Replacement>> {
        self.edits.replacements()
    }

    pub(crate) fn apply(self) -> Result<()> {
        self.edits.apply()
    }
}
