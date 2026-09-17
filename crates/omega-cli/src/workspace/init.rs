use super::{ConfigWorkspace, FileEdit, FileEdits};
use anyhow::{Context, Result, ensure};
use omega_host::Toml;
use omega_host::cargo::Manifest;

#[derive(Debug)]
pub(crate) enum InitialShell {
    Default,
    Imported {
        source: serde_json::Value,
        rust: String,
    },
}

impl InitialShell {
    pub(crate) fn import(source: &str) -> Result<Self> {
        let shell = omega_omarchy::shell::Shell::from_omarchy(source)?;
        Ok(Self::Imported {
            source: serde_json::from_str(source)?,
            rust: shell.rust_source()?,
        })
    }
}

#[derive(Debug)]
pub(crate) struct PreparedConfig<'a> {
    _workspace: &'a ConfigWorkspace,
    edits: FileEdits,
    imported: Option<serde_json::Value>,
}

impl ConfigWorkspace {
    pub(crate) fn prepare_init(&self, shell: InitialShell) -> Result<PreparedConfig<'_>> {
        let mut root = FileEdit::read(self.layout.workspace_manifest())?;
        let founded = !root.exists();
        if founded {
            root.replace(Toml::encode(&self.scaffold.workspace_manifest()?)?);
        }
        let mut editor = root.source().parse::<Manifest>()?;
        self.scaffold.complete_workspace(&mut editor)?;
        editor.ensure_member("system", "system")?;
        root.replace(editor.to_string());

        let mut system = FileEdit::read(self.layout.system_manifest())?;
        if !system.exists() {
            system.replace(Toml::encode(&self.scaffold.system_manifest()?)?);
        }
        (system.source())
            .parse::<Manifest>()?
            .package()?
            .context("missing package")?
            .name()?;
        let mut main = FileEdit::read(self.layout.system_main())?;
        let source_created = !main.exists();
        let mut edits = vec![root, system];
        let mut imported = None;
        if source_created {
            match shell {
                InitialShell::Default => {
                    ensure!(
                        !self.layout.shell_import().try_exists()?,
                        "shell_import.rs exists without system/src/main.rs; restore the entry point explicitly"
                    );
                    main.replace(self.scaffold.system_main().into());
                }
                InitialShell::Imported { source, rust } => {
                    let mut import = FileEdit::read(self.layout.shell_import())?;
                    ensure!(
                        !import.exists(),
                        "shell_import.rs already exists; restore system/src/main.rs explicitly"
                    );
                    import.replace(rust);
                    edits.push(import);
                    imported = Some(source);
                    main.replace(self.scaffold.imported_system_main().into());
                }
            }
        }
        edits.push(main);
        let mut ignore = FileEdit::read(self.layout.gitignore())?;
        let mut content = ignore.source().to_string();
        for entry in self.scaffold.gitignore().lines() {
            if !content.lines().any(|line| line == entry) {
                if !content.is_empty() && !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(entry);
                content.push('\n');
            }
        }
        ignore.replace(content);
        edits.push(ignore);
        Ok(PreparedConfig {
            _workspace: self,
            edits: FileEdits::new(edits),
            imported,
        })
    }
}

impl PreparedConfig<'_> {
    pub(crate) fn replacements(&self) -> Result<Vec<omega_host::recovery::Replacement>> {
        self.edits.replacements()
    }

    pub(crate) fn imported(&self) -> Option<&serde_json::Value> {
        self.imported.as_ref()
    }
}
