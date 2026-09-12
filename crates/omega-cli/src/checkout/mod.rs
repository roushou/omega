mod source;
pub use source::{LinkError, SourceTree};

use crate::scaffold::Scaffold;
use crate::workspace::{CargoEditor, ConfigWorkspace, FileEdit, FileEdits};
use anyhow::Result;
use omega_daemon::host::cargo::CargoConfig;
use omega_host::Toml;

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
    pub(crate) fn new(workspace: &'a ConfigWorkspace) -> Self {
        Self { workspace }
    }

    pub(crate) fn prepare(&self, source: Option<&SourceTree>) -> Result<PreparedLink<'a>> {
        let layout = self.workspace.layout();
        let mut config = FileEdit::read(layout.cargo_config())?;
        let mut editor = CargoEditor::parse(config.source())?;
        let mut generated = CargoConfig::default();
        if let Some(source) = source {
            generated.replace_patch(source.patch()?);
        }
        let patch = CargoEditor::parse(&Toml::encode(&generated)?)?;
        let specs = Scaffold::omega_crates().collect::<Vec<_>>();
        editor.link(
            &patch,
            &specs.iter().map(|s| s.package()).collect::<Vec<_>>(),
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
