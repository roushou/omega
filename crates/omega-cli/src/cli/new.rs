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

use crate::scaffold::{Scaffold, Template};
use crate::ui::{Paint, Step, Ui};
use crate::workspace::{ConfigWorkspace, PluginName};
use omega_host::Layout;

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
        let name = PluginName::parse(&self.name)?;
        let workspace = ConfigWorkspace::open(Layout::resolve())?;
        let created = workspace.prepare_plugin(name, self.template)?.apply()?;
        Self::report(ui, workspace.layout(), &created, self.template);
        Ok(())
    }

    /// What was written, and the one line omega will not write.
    fn report(ui: &mut Ui, layout: &Layout, name: &PluginName, template: Template) {
        let path = layout
            .unit_lib_src(name.unit())
            .strip_prefix(&layout.config)
            .map(|rel| rel.display().to_string())
            .unwrap_or_else(|_| layout.unit_lib_src(name.unit()).display().to_string());

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
        ui.detail(Paint::command(Scaffold::placement_hint(name, template)));
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
