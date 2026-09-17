//! Install the embedded renderer and manage Omarchy shell configuration.

use anyhow::Context;

use crate::checkout::SourceTree;
use crate::ui::{Paint, Step, Ui};
use omega_host::process::{OutputLimits, Process};
use omega_omarchy::HostShell;
use omega_omarchy::{Installed, Renderer};
use std::time::Duration;

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
    /// Compare installed files and running renderer builds with this omega.
    Status,
    /// Take it away again.
    Uninstall,
}

#[derive(Debug, clap::Args)]
struct Install {
    /// Symlink renderer files from a checkout. Defaults to `$OMEGA_SOURCE`
    /// or this binary's source tree. Restart the shell after editing linked files.
    #[arg(long, num_args = 0..=1, default_missing_value = "", value_name = "PATH")]
    link: Option<String>,
    /// Install files without restarting the shell or verifying activation.
    #[arg(long)]
    no_restart: bool,
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
                let before = if install.no_restart {
                    Ok(crate::renderer::Snapshot {
                        active: Vec::new(),
                        placements: Vec::new(),
                    })
                } else {
                    crate::renderer::RendererStatus::read().await
                };
                let no_restart = install.no_restart;
                let linked = install.link.is_some();
                Self::install(install, shell, ui)?;
                if no_restart {
                    ui.warn("renderer files installed; running QML activation was not verified");
                    ui.next(shell.reload_command());
                } else {
                    Self::restart(shell, ui).await?;
                    match before {
                        Ok(before) if !linked => crate::renderer::RendererStatus::verify_activation(&before, ui).await?,
                        Ok(_) => ui.warn("linked renderer restarted; source build identity is unverified"),
                        Err(error) => ui.warn(format!("shell restarted; running renderer unverified because daemon status was unavailable: {error}")),
                    }
                }
                // Renderer installation does not change widget placements.
                ui.next(&shell.enable_command(Renderer::VIEW.id));
                Ok(())
            }
            Action::Status => {
                Self::status(shell, ui)?;
                match crate::renderer::RendererStatus::read().await {
                    Ok(attachments) => crate::renderer::RendererStatus::show(
                        &attachments.active,
                        &attachments.placements,
                        ui,
                    ),
                    Err(error) => ui.warn(format!("running renderer unverified: {error}")),
                }
                Ok(())
            }
            Action::Uninstall => Self::uninstall(shell, ui).await,
            Action::Adopt | Action::Diff | Action::Apply { .. } => unreachable!("handled above"),
        }
    }

    fn adopt(ui: &mut Ui) -> anyhow::Result<()> {
        let layout = omega_host::Layout::resolve();
        let source = std::fs::read_to_string(&layout.shell_config)?;
        let shell = omega_omarchy::shell::Shell::from_omarchy(&source)
            .with_context(|| format!("cannot import {}", layout.shell_config.display()))?;
        let target = layout.shell_import();
        anyhow::ensure!(
            !target.exists(),
            "{} already exists; review or rename it before importing again",
            target.display()
        );
        omega_host::AtomicFile::at(&target).write(shell.rust_source()?.as_bytes())?;
        omega_omarchy::installation::ShellInstallation::new(&layout)
            .adopt(&serde_json::from_str(&source)?)?;
        ui.step(Step::Created, Paint::path(&target));
        ui.next("add mod shell_import; and .with(shell_import::shell()?)? to your document, replacing legacy .bar(...) declarations");
        ui.detail("The desktop is unchanged. Review the import, then run omega build.");
        Ok(())
    }

    fn diff(ui: &mut Ui) -> anyhow::Result<()> {
        let layout = omega_host::Layout::resolve();
        let installer = omega_omarchy::installation::ShellInstallation::new(&layout);
        let ownership = installer.inspect()?;
        ui.step(Step::Checking, format!("shell ownership: {ownership:?}"));
        let generation = omega_host::Generations::new(&layout)
            .pin_current()?
            .context("nothing built")?;
        let document = omega_document::DocumentFile::of(generation.layout()).read()?;
        let compiled = omega_omarchy::shell::CompiledShell::of(&document)?
            .context("this generation declares no shell")?;
        let current = installer.read()?;
        if current.as_ref() == Some(compiled.config()) {
            ui.step(Step::Checked, "shell configuration matches");
        } else {
            ui.shell_diff(current.as_ref(), compiled.config());
            ui.detail("Changes shown from the current file to the built configuration.");
        }
        use omega_omarchy::installation::InstallationState;
        match ownership {
            InstallationState::Unmanaged if current.is_some() => {
                ui.detail(
                    "Adopt the existing shell configuration before applying a generated one.",
                );
                ui.next("omega shell adopt");
            }
            InstallationState::ModifiedExternally => {
                ui.detail("To keep external edits, copy them into Rust and rebuild first.");
                ui.next("omega shell apply --overwrite");
            }
            _ if current.as_ref() != Some(compiled.config()) => ui.next("omega shell apply"),
            _ => {}
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
                    // Linked renderer files require an explicit shell rescan after edits.
                    ui.detail(Paint::dim(format!(
                        "not watched through a link — {} after editing",
                        shell.reload_command()
                    )));
                }
                None => {
                    let recovery =
                        omega_host::recovery::RecoveryStore::new(&omega_host::Layout::resolve());
                    let installed = renderer.install(&plugins, &recovery)?;
                    if let Some(recovery) = &installed.recovery {
                        ui.detail(format!(
                            "Recovery record: {}",
                            Paint::path(&recovery.record)
                        ));
                    } else {
                        ui.detail(
                            "Installed files already match; no backup or replacement needed.",
                        );
                    }
                    let dir = installed.target;
                    ui.step(
                        if installed.recovery.is_some() {
                            Step::Installed
                        } else {
                            Step::Checked
                        },
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

        Ok(())
    }

    async fn restart(shell: HostShell, ui: &mut Ui) -> anyhow::Result<()> {
        crate::renderer::RendererStatus::restart(shell).await?;
        ui.step(
            Step::Restarted,
            "Omarchy shell; QML component cache cleared",
        );
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

        if wanted {
            ui.next("omega shell install");
        } else {
            ui.step(Step::Checked, "installed renderer files match this CLI");
        }
        Ok(())
    }

    async fn uninstall(shell: HostShell, ui: &mut Ui) -> anyhow::Result<()> {
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

        Self::rescan(shell.rescan_command().into(), ui).await;
        Ok(())
    }

    /// The checkout a `--link` means: the one named, else the one found.
    fn checkout(path: &str) -> anyhow::Result<SourceTree> {
        if path.is_empty() {
            SourceTree::detect()?.with_context(|| {
                format!(
                    "no omega checkout to link — name one, or set {}",
                    Paint::name(SourceTree::ENV)
                )
            })
        } else {
            Ok(SourceTree::at(path)?)
        }
    }

    /// Report rescan failures without undoing completed file removal.
    async fn rescan(command: tokio::process::Command, ui: &mut Ui) {
        let result = Process::new(command)
            .timeout(Duration::from_secs(10))
            .capture(OutputLimits {
                stdout: 64 * 1024,
                stderr: 64 * 1024,
            })
            .await;
        match result {
            Ok(output) if output.status.success() => {}
            Ok(output) => ui.warn(format!(
                "shell plugin rescan failed ({}): {}; the shell will refresh its plugins when it restarts",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Err(error) => ui.warn(format!(
                "shell plugin rescan failed: {error}; the shell will refresh its plugins when it restarts"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::process::Command;

    struct Rescan;

    impl Rescan {
        fn command(script: &str) -> Command {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script]);
            command
        }
    }

    #[tokio::test]
    async fn rescan_reports_actual_failures_as_warnings() {
        let (mut ui, transcript) = Ui::recording();
        ShellCmd::rescan(Rescan::command("exit 0"), &mut ui).await;
        assert!(transcript.err().is_empty());

        ShellCmd::rescan(
            Rescan::command("printf 'rescan rejected' >&2; exit 9"),
            &mut ui,
        )
        .await;
        let diagnostic = transcript.err();
        assert!(diagnostic.contains("Warning"), "{diagnostic}");
        assert!(diagnostic.contains("exit status: 9"), "{diagnostic}");
        assert!(diagnostic.contains("rescan rejected"), "{diagnostic}");
        assert!(diagnostic.contains("when it restarts"), "{diagnostic}");

        ShellCmd::rescan(Command::new("/dev/null/missing"), &mut ui).await;
        assert!(transcript.err().contains("could not start subprocess"));
        assert!(transcript.out().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn a_stalled_rescan_returns_a_warning_after_ten_seconds() {
        let (mut ui, transcript) = Ui::recording();
        ShellCmd::rescan(Rescan::command("exec /bin/sleep 60"), &mut ui).await;
        assert!(transcript.err().contains("timed out after 10s"));
    }
}
