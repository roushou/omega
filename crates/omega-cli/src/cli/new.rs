//! Scaffold a plugin, command host, or library and connect its Cargo dependencies.
//!
//! Placement belongs to the config author. Print its declaration without changing
//! the system's layout.

use crate::scaffold::{Scaffold, Template};
use crate::ui::{Paint, Step, Ui};
use crate::workspace::ConfigWorkspace;
use omega_host::Layout;
use omega_host::package::PackageName;

/// Scaffold a plugin, reusable command host, or shared library.
#[derive(Debug, clap::Args)]
pub struct NewCmd {
    #[arg(value_name = "NAME")]
    pub package_name: PackageName,
    /// Create a shared Rust library, which Omega never supervises.
    #[arg(long, conflicts_with = "template")]
    pub lib: bool,
    /// Create a reusable command library and executable under commands/<name>.
    #[arg(long, conflicts_with_all = ["lib", "template", "into"])]
    pub command_host: bool,
    /// Add the library to a member (system, plugins/name, commands/name, crates/name).
    #[arg(long, requires = "lib", value_name = "MEMBER")]
    pub into: Vec<std::path::PathBuf>,
    /// Choose the plugin's starting point.
    #[arg(long, value_enum, default_value = "minimal")]
    pub template: Template,
}

impl NewCmd {
    pub fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let workspace = ConfigWorkspace::open(Layout::resolve())?;
        if self.lib {
            let created = workspace
                .prepare_library(self.package_name, &self.into)?
                .apply()?;
            ui.step(
                Step::Created,
                Paint::path(workspace.layout().library_src_dir(&created)),
            );
            for consumer in self.into {
                ui.step(
                    Step::Linked,
                    format!("{} → {}", consumer.display(), created.package()),
                );
            }
            return Ok(());
        }
        if self.command_host {
            let created = workspace.prepare_command_host(self.package_name)?.apply()?;
            ui.step(
                Step::Created,
                Paint::path(workspace.layout().command_src_dir(&created)),
            );
            ui.step(Step::Linked, format!("system → {}", created.package()));
            ui.step(
                Step::Next,
                format!(
                    "add this call to the Document builder in {}:",
                    Paint::path(workspace.layout().system_main())
                ),
            );
            ui.detail(Paint::command(Scaffold::command_host_hint(&created)));
            ui.detail(
                "The default deployment starts on the first call and keeps the process running.",
            );
            ui.next("omega build");
            ui.next(&format!("omega run {}.echo hello", created.package()));
            return Ok(());
        }
        let created = workspace
            .prepare_plugin(self.package_name, self.template)?
            .apply()?;
        Self::report(ui, workspace.layout(), &created, self.template);
        Ok(())
    }

    /// What was written, and the one line omega will not write.
    fn report(ui: &mut Ui, layout: &Layout, package_name: &PackageName, template: Template) {
        let path = layout
            .plugin_lib_src(package_name.plugin())
            .strip_prefix(&layout.config)
            .map(|rel| rel.display().to_string())
            .unwrap_or_else(|_| {
                layout
                    .plugin_lib_src(package_name.plugin())
                    .display()
                    .to_string()
            });

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
        ui.detail(Paint::command(Scaffold::placement_hint(
            package_name,
            template,
        )));
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
