//! `omega dev`: run a unit from source, against the daemon that is already
//! running.
//!
//! Without this a unit cannot be run at all. A unit proves itself with a
//! token only the daemon issues, so a process started by hand is refused —
//! and the only way to see a change was a release build of the whole
//! workspace followed by a reload. This is the loop that replaces it: the
//! daemon hands over the unit's identity, this process runs the debug binary
//! in its place, and every save rebuilds and restarts it.
//!
//! The adoption lasts exactly as long as this command does. Whatever ends it
//! — Ctrl-C, a closed terminal, a crash — closes the connection, and the
//! daemon puts the built unit back.

use anyhow::{Context, bail};

use omega_daemon::host::{Changes, Recursion};
use omega_proto::{Handshake, Socket};
use omega_proto::{Layout, Profile, UnitName};

use crate::cargo::Cargo;
use crate::operator::Operator;
use crate::ui::{Paint, Step, Ui};

/// Run a unit from source in place of the built one.
#[derive(Debug, clap::Args)]
pub struct DevCmd {
    /// The unit to take over.
    pub unit: String,
}

impl DevCmd {
    /// The profile the inner loop compiles with. Nobody waiting on a save
    /// wants an optimiser.
    const PROFILE: Profile = Profile::Debug;

    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let name = UnitName::parse(&self.unit)?;
        let layout = Layout::resolve();

        let source = layout.unit_src_dir(&name);
        if !source.exists() {
            bail!(
                "no unit {name} in {} — scaffold one with {}",
                Paint::path(layout.units_dir()),
                Paint::command(format!("omega init {name}"))
            );
        }

        let cargo = Cargo::new(&layout);
        cargo.build_unit(Self::PROFILE, &name).await?;

        let mut attached = Operator::new().attach().await.with_context(|| {
            format!(
                "no daemon to develop against — start one with {}",
                Paint::command("omega daemon")
            )
        })?;

        // The unit's own directory: cargo writes to the cache, so a rebuild
        // cannot trip the watch that started it.
        let mut changes = Changes::watch(&[source.as_path()], Recursion::Recursive)?;

        let program = layout.compiled_binary(Self::PROFILE, &name);
        let socket = Socket::resolve();

        loop {
            // A fresh token per run: a token binds to the first process that
            // presents it, and every restart is a different process.
            let token = attached.adopt(name.as_str()).await?;
            let mut child = Self::spawn(&program, &socket, &token)
                .with_context(|| format!("cannot run {}", Paint::path(&program)))?;

            ui.step(
                Step::Adopted,
                format!(
                    "{} — the daemon is running this process instead",
                    Paint::name(&name)
                ),
            );
            ui.step(Step::Watching, Paint::path(&source));

            let restart = tokio::select! {
                exit = child.wait() => {
                    // The unit stopped on its own. Waiting for a change
                    // rather than respawning is the difference between a dev
                    // loop and a crash loop: the fix is a save away.
                    Self::exited(ui, &name, exit);
                    changes.next().await.is_some()
                }
                Some(()) = changes.next() => {
                    Self::stop(&mut child).await;
                    true
                }
                // The daemon went away; there is nothing left to develop
                // against.
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
            ui.step(Step::Changed, format!("rebuilding {}", Paint::name(&name)));
            if let Err(e) = cargo.build_unit(Self::PROFILE, &name).await {
                // A build that fails leaves the loop running: the next save
                // is the fix, and exiting would throw away the adoption.
                ui.error(&e.into());
                if changes.next().await.is_none() {
                    break;
                }
            }
        }

        // Dropping the connection is what ends the adoption; saying so is
        // just manners.
        ui.step(
            Step::Released,
            format!("{} — the built unit runs again", Paint::name(&name)),
        );
        Ok(())
    }

    /// The unit, started the way the supervisor would start it: the daemon's
    /// socket, and the token it is this unit by. Its output is inherited,
    /// because reading it is the reason to be here.
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

    fn exited(ui: &mut Ui, name: &UnitName, exit: std::io::Result<std::process::ExitStatus>) {
        match exit {
            Ok(status) if status.success() => {
                ui.step(Step::Done, format!("{} exited", Paint::name(name)))
            }
            Ok(status) => ui.step(
                Step::Failed,
                format!("{} {}", Paint::name(name), Paint::problem(status)),
            ),
            Err(e) => ui.warn(format!("cannot wait for {name}: {e}")),
        }
    }

    /// Ask, then insist — the same bargain the supervisor offers a unit.
    async fn stop(child: &mut tokio::process::Child) {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}
