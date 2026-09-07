//! `omega shell`: the renderer that draws what units publish.
//!
//! A unit's view tree becomes pixels inside the host shell's process, so
//! omega ships a plugin for it. This installs that plugin out of the binary,
//! which is what keeps it in step with the daemon it reads: no checkout to
//! copy from, no version to keep in your head, and no way to be running a
//! renderer that is older than the wire format it is reading.

use anyhow::Context;

use crate::scaffold::SourceTree;
use crate::shell::{HostShell, Installed, Renderer};
use crate::ui::{Paint, Step, Ui};

/// Install and inspect the shell plugin that draws omega's views.
#[derive(Debug, clap::Args)]
pub struct ShellCmd {
    #[command(subcommand)]
    action: Option<Action>,
}

#[derive(Debug, clap::Subcommand)]
enum Action {
    /// Put the renderer where the shell will find it.
    Install(Install),
    /// What is installed, and whether it matches this omega.
    Status,
    /// Take it away again.
    Uninstall,
}

#[derive(Debug, clap::Args)]
struct Install {
    /// Symlink a checkout instead of copying it in, so edits to the QML
    /// reload without reinstalling. Defaults to `$OMEGA_SOURCE`, else the
    /// tree this binary was built from.
    #[arg(long, num_args = 0..=1, default_missing_value = "", value_name = "PATH")]
    link: Option<String>,
}

impl ShellCmd {
    pub fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let shell = HostShell::detect().with_context(|| {
            format!(
                "no shell to install into — omega draws through a host shell's plugins, and this machine has none omega knows. Name the directory with {}",
                Paint::name(HostShell::ENV)
            )
        })?;

        match self.action.unwrap_or(Action::Status) {
            Action::Install(install) => {
                Self::install(install, shell, ui)?;
                // Where a widget sits in the bar is the person's layout, and
                // a command that rearranges someone's screen because they
                // installed something is a command they stop trusting.
                ui.next(&shell.enable_command(Renderer::VIEW.id));
                Ok(())
            }
            Action::Status => Self::status(shell, ui),
            Action::Uninstall => Self::uninstall(shell, ui),
        }
    }

    /// Install the renderer as part of setting omega up, and put it on the
    /// bar — which is what somebody running `omega init` is asking for.
    ///
    /// A machine with no host shell is not a failed setup: the daemon runs,
    /// and there is simply nowhere to draw yet.
    pub(crate) fn setup(ui: &mut Ui) -> anyhow::Result<()> {
        let Some(shell) = HostShell::detect() else {
            ui.warn("no host shell here — nothing will be drawn until there is one");
            return Ok(());
        };

        Self::install(Install { link: None }, shell, ui)?;

        match shell.enable(Renderer::VIEW.id) {
            Ok(output) if output.status.success() => ui.step(
                Step::Enabled,
                format!("{} in the bar", Paint::name(Renderer::VIEW.id)),
            ),
            _ => ui.next(&shell.enable_command(Renderer::VIEW.id)),
        }
        Ok(())
    }

    fn install(install: Install, shell: HostShell, ui: &mut Ui) -> anyhow::Result<()> {
        let plugins = shell.plugins();

        for renderer in Renderer::ALL {
            match &install.link {
                Some(path) => {
                    let tree = Self::checkout(path)?;
                    renderer.link(&plugins, tree.root())?;
                    ui.step(
                        Step::Linked,
                        format!(
                            "{} — {} draws from {}",
                            Paint::name(renderer.id),
                            shell.name(),
                            Paint::path(tree.root().join(renderer.source))
                        ),
                    );
                    // The shell finds a link but does not watch through one,
                    // and somebody expecting their edit on screen deserves to
                    // hear that from the command that made it so.
                    ui.detail(Paint::dim(format!(
                        "not watched through a link — {} after editing",
                        shell.reload_command()
                    )));
                }
                None => {
                    let dir = renderer.install(&plugins)?;
                    ui.step(
                        Step::Installed,
                        format!(
                            "{} {} in {}",
                            Paint::name(renderer.id),
                            Paint::dim(Renderer::VERSION),
                            Paint::path(dir.parent().unwrap_or(&plugins))
                        ),
                    );
                }
            }
        }

        Self::rescan(shell, ui);
        Ok(())
    }

    fn status(shell: HostShell, ui: &mut Ui) -> anyhow::Result<()> {
        let plugins = shell.plugins();
        ui.step(
            Step::Checking,
            format!("{} — {}", shell.name(), Paint::path(&plugins)),
        );

        let width = Ui::width(Renderer::ALL.iter().map(|renderer| renderer.id));
        let mut wanted = false;

        for renderer in Renderer::ALL {
            let id = Ui::column(renderer.id, width);
            match renderer.installed(&plugins) {
                Installed::Current => ui.item(
                    true,
                    format!("{}  {}", Paint::name(id), Paint::dim(Renderer::VERSION)),
                ),
                Installed::Linked(target) => ui.item(
                    true,
                    format!(
                        "{}  {} {}",
                        Paint::name(id),
                        Paint::dim("linked to"),
                        Paint::path(target)
                    ),
                ),
                Installed::Missing => {
                    wanted = true;
                    ui.item(false, format!("{}  {}", Paint::name(id), "not installed"));
                }
                stale => {
                    wanted = true;
                    ui.item(
                        false,
                        format!(
                            "{}  {}",
                            Paint::name(id),
                            stale.difference().unwrap_or_default()
                        ),
                    );
                }
            }
        }

        // Not a failure: a machine that draws nothing on its bar is entitled
        // to have no renderer installed. It is a thing to do about it.
        match wanted {
            true => ui.next("omega shell install"),
            false => ui.step(Step::Checked, "the shell draws what this omega speaks"),
        }
        Ok(())
    }

    fn uninstall(shell: HostShell, ui: &mut Ui) -> anyhow::Result<()> {
        let plugins = shell.plugins();

        for renderer in Renderer::ALL {
            match renderer.uninstall(&plugins)? {
                Some(dir) => ui.step(
                    Step::Removed,
                    format!("{} from {}", Paint::name(renderer.id), Paint::path(dir)),
                ),
                None => ui.step(
                    Step::Done,
                    format!("{} was not installed", Paint::name(renderer.id)),
                ),
            }
        }

        Self::rescan(shell, ui);
        Ok(())
    }

    /// The checkout a `--link` means: the one named, else the one found.
    fn checkout(path: &str) -> anyhow::Result<SourceTree> {
        match path.is_empty() {
            false => Ok(SourceTree::at(path)?),
            true => SourceTree::detect().with_context(|| {
                format!(
                    "no omega checkout to link — name one, or set {}",
                    Paint::name(SourceTree::ENV)
                )
            }),
        }
    }

    /// Ask the shell to look again, and say so only when it could not be
    /// asked: a shell that is not running is not a failed install, and the
    /// files are on disk either way.
    fn rescan(shell: HostShell, ui: &mut Ui) {
        match shell.rescan() {
            Ok(output) if output.status.success() => {}
            _ => ui.warn(format!(
                "{} was not listening — it will find this when it starts",
                shell.name()
            )),
        }
    }
}
