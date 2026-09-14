mod source;
pub use source::{LinkError, SourceTree};

use crate::scaffold::Scaffold;
use crate::ui::Paint;
use crate::workspace::{CargoEditor, ConfigWorkspace, FileEdit, FileEdits};
use anyhow::Result;
use omega_host::workspace::cargo::CargoConfig;
use omega_host::{Layout, Toml};

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
    /// Why a build might have failed to resolve omega at all.
    ///
    /// A config that is not linked and asks for crates nobody has published
    /// fails in cargo's words, which name a package and say nothing about
    /// omega. This is the sentence that was missing.
    pub(crate) fn unlinked(layout: &Layout) -> Option<String> {
        let linked = layout
            .file::<CargoConfig>(())
            .read_or_default()
            .ok()
            .and_then(|config| config.patched().map(|patched| !patched.is_empty()))
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
        let mut editor = CargoEditor::parse(config.source())?;
        let root_source = FileEdit::read(layout.workspace_manifest())?;
        let root_editor = CargoEditor::parse(root_source.source())?;
        let preview = Scaffold::PREVIEW_DEPENDENCIES
            .iter()
            .any(|s| root_editor.has_workspace_dependency(s.name));
        let mut generated = CargoConfig::default();
        if let Some(source) = source {
            generated.replace_patch(source.patch(preview)?);
        }
        let patch = CargoEditor::parse(&Toml::encode(&generated)?)?;
        let specs = Scaffold::omega_crates()
            .chain(Scaffold::PREVIEW_DEPENDENCIES.iter().filter(|_| preview))
            .collect::<Vec<_>>();
        editor.link(
            &patch,
            &Scaffold::omega_crates()
                .chain(Scaffold::PREVIEW_DEPENDENCIES)
                .map(|s| s.package())
                .collect::<Vec<_>>(),
        )?;
        config.replace(editor.finish());
        let mut edits = Vec::new();
        if let Some(source) = source {
            let mut root = FileEdit::read(layout.workspace_manifest())?;
            let mut editor = CargoEditor::parse(root.source())?;
            editor.require(
                &specs.iter().map(|s| s.name).collect::<Vec<_>>(),
                &source.version()?,
            )?;
            root.replace(editor.finish());
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
    pub(crate) fn apply(self) -> Result<()> {
        self.edits.apply()
    }
}
