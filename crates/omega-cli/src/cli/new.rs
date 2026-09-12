//! `omega new`: write a plugin.
//!
//! Not `add`: `omarchy plugin add <git-url>` already means *install somebody
//! else's plugin*, and omega will want that word for the same thing one day.
//! This is cargo's `new` — it makes something that did not exist.
//!
//! It stops one step short of putting the widget on screen, and that step is
//! deliberate. Adding the plugin to the workspace is bookkeeping, so it is
//! done; deciding which bar it belongs in and what to configure it with is
//! not, so the line is printed rather than written into a file the author
//! owns.

use anyhow::bail;

use omega_daemon::host::cargo::{CargoManifest, CargoSlot};
use omega_host::{AtomicFile, Layout};
use omega_proto::UnitName;

use crate::scaffold::{Scaffold, Template};
use crate::ui::{Paint, Step, Ui};

/// Scaffold a plugin into `~/.config/omega/units/<name>`.
#[derive(Debug, clap::Args)]
pub struct NewCmd {
    pub name: String,
    /// Choose the plugin's starting point.
    #[arg(long, value_enum, default_value = "minimal")]
    pub template: Template,
}

impl NewCmd {
    pub fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let name = UnitName::parse(&self.name)?;
        let layout = Layout::resolve();
        let scaffold = Scaffold::new();

        if !layout.workspace_manifest().exists() {
            bail!(
                "{} is not a config yet — set this machine up with {}",
                Paint::path(&layout.config),
                Paint::command("omega init")
            );
        }

        let unit_dir = layout.unit_src_dir(&name);
        if unit_dir.exists() {
            bail!("{name} already exists at {}", unit_dir.display());
        }

        // The plugin: a cargo manifest, and the one file that is the plugin.
        layout
            .file::<CargoManifest>(CargoSlot::Unit(&name))
            .write(&scaffold.unit_crate_manifest(&name))?;
        AtomicFile::at(layout.unit_lib_src(&name)).write(self.template.library().as_bytes())?;
        AtomicFile::at(layout.unit_main_src(&name)).write(scaffold.unit_main(&name)?.as_bytes())?;

        let workspace_file = layout.file::<CargoManifest>(CargoSlot::Workspace);
        let mut manifest = workspace_file.read()?;
        let workspace = manifest
            .workspace
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("the config manifest has no [workspace]"))?;
        let mut changed = Scaffold::ensure_edition(workspace);
        // Existing membership globs remain authoritative.
        if !workspace.member_dirs(&layout.config)?.contains(&unit_dir) {
            workspace.members.push(format!("units/{name}"));
            changed = true;
        }
        if changed {
            workspace_file.write(&manifest)?;
        }

        // The config plane depends on every plugin it configures — that is
        // what makes a plugin's settings a type here rather than a map of
        // strings. A path between two crates of one workspace is bookkeeping,
        // so omega keeps it rather than asking.
        layout
            .file::<CargoManifest>(CargoSlot::System)
            .edit(|manifest| {
                manifest
                    .dependencies
                    .insert(name.as_str(), Scaffold::depends_on(&name));
            })?;

        Self::report(ui, &layout, &name);
        Ok(())
    }

    /// What was written, and the one line omega will not write.
    fn report(ui: &mut Ui, layout: &Layout, name: &UnitName) {
        let path = layout
            .unit_lib_src(name)
            .strip_prefix(&layout.config)
            .map(|rel| rel.display().to_string())
            .unwrap_or_else(|_| layout.unit_lib_src(name).display().to_string());

        ui.step(
            Step::Created,
            format!(
                "{}  {}",
                path,
                Paint::dim("the plugin: what it needs, and what it draws")
            ),
        );

        ui.step(
            Step::Next,
            "add this expression to Bar::left(...), Bar::center(...), or Bar::right(...) in your Rust shell layout:",
        );
        ui.detail(Paint::command(Scaffold::placement_hint(name)));
        ui.detail(format!(
            "Start at {}; follow its shell module if the layout lives elsewhere.",
            Paint::path(layout.system_main())
        ));
        if layout.shell_import().exists() {
            ui.detail(format!(
                "Imported layout: {} (if included by your document).",
                Paint::path(layout.shell_import())
            ));
        }
        ui.detail("Building starts the plugin; placing its widget is what makes it visible.");
        ui.next("omega build");
    }
}
