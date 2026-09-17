//! Run a plugin from source against the current daemon, rebuilding on changes.
//! The daemon lends its supervised identity for the lifetime of the operator
//! connection and restores the built plugin when that connection closes.

use anyhow::{Context, bail};

use omega_host::cargo::{Cargo, PackageSpec, Selection};
use omega_host::fs::{Changes, Recursion};
use omega_host::{Layout, Profile};
use omega_proto::PluginName;
use omega_proto::{Handshake, Socket};

use crate::build::Build;
use crate::operator::Operator;
use crate::ui::{Paint, Step, Ui};

/// Run a plugin from source in place of the built one.
#[derive(Debug, clap::Args)]
pub struct DevCmd {
    /// The plugin to take over.
    #[arg(value_name = "PLUGIN")]
    pub plugin_name: PluginName,
}

impl DevCmd {
    /// The profile the inner loop compiles with. Nobody waiting on a save
    /// wants an optimiser.
    const PROFILE: Profile = Profile::Debug;

    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let plugin_name = self.plugin_name;
        let layout = Layout::resolve();

        let source = layout.plugin_src_dir(&plugin_name);
        if !source.exists() {
            bail!(
                "no plugin {plugin_name} in {} — scaffold one with {}",
                Paint::path(layout.plugins_dir()),
                Paint::command(format!("omega new {plugin_name}"))
            );
        }

        Self::build(&layout, &plugin_name).await?;

        let mut attached = Operator::new().attach().await.with_context(|| {
            format!(
                "no daemon to develop against — start one with {}",
                Paint::command("omega daemon")
            )
        })?;

        // Shared library and workspace dependency edits also invalidate this plugin.
        let mut changes = Changes::watch(&[layout.config.as_path()], Recursion::Recursive)?;

        let program = layout.compiled_binary(Self::PROFILE, &plugin_name);
        let socket = Socket::resolve();

        loop {
            // A fresh token per run: a token binds to the first process that
            // presents it, and every restart is a different process.
            let token = attached.adopt(&plugin_name).await?;
            let mut child = Self::spawn(&program, &socket, &token)
                .with_context(|| format!("cannot run {}", Paint::path(&program)))?;

            ui.step(
                Step::Adopted,
                format!(
                    "{} — the daemon is running this process instead",
                    Paint::name(&plugin_name)
                ),
            );
            ui.step(Step::Watching, Paint::path(&source));

            let restart = tokio::select! {
                exit = child.wait() => {
                    // Wait for a source change after process exit instead of repeatedly respawning.
                    Self::exited(ui, &plugin_name, exit);
                    changes.next().await.is_some()
                }
                Some(()) = changes.next() => {
                    Self::stop(&mut child).await;
                    true
                }
                _ = attached.hold() => {
                    Self::stop(&mut child).await;
                    ui.warn("the daemon closed the connection");
                    false
                }
                _ = tokio::signal::ctrl_c() => {
                    Self::stop(&mut child).await;
                    false
                }
            };

            if !restart {
                break;
            }

            ui.blank();
            ui.step(
                Step::Changed,
                format!("rebuilding {}", Paint::name(&plugin_name)),
            );
            if let Err(e) = Self::build(&layout, &plugin_name).await {
                // A build that fails leaves the loop running: the next save
                // is the fix, and exiting would throw away the adoption.
                ui.error(&e);
                if changes.next().await.is_none() {
                    break;
                }
            }
        }

        ui.step(
            Step::Released,
            format!(
                "{} — the built plugin runs again",
                Paint::name(&plugin_name)
            ),
        );
        Ok(())
    }

    async fn build(layout: &Layout, plugin_name: &PluginName) -> anyhow::Result<()> {
        let _workspace = crate::workspace::ConfigWorkspace::open(layout.clone())?;
        Cargo::new(&layout.config)
            .build(Build::request(
                layout,
                Self::PROFILE,
                Selection::Package(PackageSpec::try_from(plugin_name.as_str())?),
            ))
            .await?;
        Ok(())
    }

    /// Spawn with the adopted token and daemon socket; inherit terminal output.
    fn spawn(
        program: &std::path::Path,
        socket: &Socket,
        token: &str,
    ) -> std::io::Result<tokio::process::Child> {
        tokio::process::Command::new(program)
            .env("OMEGA_SOCKET", socket.path())
            .env(Handshake::TOKEN_ENV, token)
            .kill_on_drop(true)
            .spawn()
    }

    fn exited(
        ui: &mut Ui,
        plugin_name: &PluginName,
        exit: std::io::Result<std::process::ExitStatus>,
    ) {
        match exit {
            Ok(status) if status.success() => {
                ui.step(Step::Done, format!("{} exited", Paint::name(plugin_name)))
            }
            Ok(status) => ui.step(
                Step::Failed,
                format!("{} {}", Paint::name(plugin_name), Paint::problem(status)),
            ),
            Err(e) => ui.warn(format!("cannot wait for {plugin_name}: {e}")),
        }
    }

    /// Ask, then insist — the same bargain the supervisor offers a plugin.
    async fn stop(child: &mut tokio::process::Child) {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}
