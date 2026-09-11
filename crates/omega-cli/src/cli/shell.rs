//! `omega shell`: the renderer that draws what units publish.
//!
//! A unit's view tree becomes pixels inside the host shell's process, so
//! omega ships a plugin for it. This installs that plugin out of the binary,
//! which is what keeps it in step with the daemon it reads: no checkout to
//! copy from, no version to keep in your head, and no way to be running a
//! renderer that is older than the wire format it is reading.

use anyhow::Context;

use crate::scaffold::SourceTree;
use crate::ui::{Paint, Step, Ui};
use omega_renderer::{HostShell, Installed, Renderer};

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
    /// Import the existing configuration into a reviewable Rust module and establish ownership.
    Adopt,
    /// Compare the published shell configuration with Omarchy's file.
    Diff,
    /// Ask the daemon to apply the published shell configuration.
    Apply {
        /// Acknowledge external edits and replace them with the Rust configuration.
        #[arg(long)]
        overwrite: bool,
    },
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
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let action = self.action.unwrap_or(Action::Status);
        match action {
            Action::Adopt => return Self::adopt(ui),
            Action::Diff => return Self::diff(ui),
            Action::Apply { overwrite } => {
                crate::operator::Operator::new()
                    .apply_shell(overwrite)
                    .await?;
                ui.step(
                    Step::Installed,
                    "shell configuration from the published generation",
                );
                return Ok(());
            }
            _ => {}
        }
        let shell = HostShell::detect().with_context(|| {
            format!(
                "no shell to install into — omega draws through a host shell's plugins, and this machine has none omega knows. Name the directory with {}",
                Paint::name(HostShell::ENV)
            )
        })?;

        match action {
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
            Action::Adopt | Action::Diff | Action::Apply { .. } => unreachable!("handled above"),
        }
    }

    fn adopt(ui: &mut Ui) -> anyhow::Result<()> {
        let layout = omega_host::Layout::resolve();
        let source = std::fs::read_to_string(&layout.shell_config)?;
        let shell = omega_document::shell::Shell::from_omarchy(&source)?;
        let target = layout.shell_import();
        anyhow::ensure!(
            !target.exists(),
            "{} already exists; review or rename it before importing again",
            target.display()
        );
        omega_host::AtomicFile::at(&target).write(shell.rust_source()?.as_bytes())?;
        omega_host::shell::ShellInstallation::new(&layout)
            .adopt(&serde_json::from_str(&source)?)?;
        ui.step(Step::Created, Paint::path(&target));
        ui.next("add mod shell_import; and .shell(shell_import::shell()?)? to your document, replacing legacy .bar(...) declarations");
        ui.detail("The desktop is unchanged. Review the import, then run omega build.");
        Ok(())
    }

    fn diff(ui: &mut Ui) -> anyhow::Result<()> {
        let layout = omega_host::Layout::resolve();
        let installer = omega_host::shell::ShellInstallation::new(&layout);
        ui.step(
            Step::Checking,
            format!("shell ownership: {:?}", installer.inspect()?),
        );
        let generation = omega_host::Generations::new(&layout)
            .pin_current()?
            .context("nothing built")?;
        let document = omega_document::DocumentFile::of(generation.layout()).read()?;
        let compiled = omega_document::shell::CompiledShell::of(&document)?
            .context("this generation declares no shell")?;
        let current = installer.read()?;
        if current.as_ref() == Some(compiled.config()) {
            ui.step(Step::Checked, "shell configuration matches");
        } else {
            ui.step(Step::Checking, "current shell configuration");
            ui.detail(serde_json::to_string_pretty(&current)?);
            ui.step(Step::Checking, "generated shell configuration");
            ui.detail(compiled.encode()?);
        }
        Ok(())
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

        ui.next("declare widget placements in your Rust shell layout, then run omega build");
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
        if wanted {
            ui.next("omega shell install");
        } else {
            ui.step(Step::Checked, "the shell draws what this omega speaks");
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
        if path.is_empty() {
            SourceTree::detect().with_context(|| {
                format!(
                    "no omega checkout to link — name one, or set {}",
                    Paint::name(SourceTree::ENV)
                )
            })
        } else {
            Ok(SourceTree::at(path)?)
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
