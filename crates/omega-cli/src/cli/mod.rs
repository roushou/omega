mod build;
mod check;
mod clean;
pub(crate) mod daemon;
mod dev;
mod init;
pub(crate) mod link;
mod logs;
mod new;
mod restart;
mod rollback;
mod run;
pub(crate) mod shell;
mod status;

use std::io::IsTerminal;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::ui::Ui;

/// The `omega` CLI. One variant per command.
#[derive(Parser)]
#[command(name = "omega", version, about = "Omega")]
#[derive(Debug)]
pub struct Cli {
    /// Increase output verbosity (repeat for more: -v = debug, -vv = trace).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }

    /// Log verbosity: 0 = info, 1 = debug, 2+ = trace.
    pub fn verbosity(&self) -> u8 {
        self.verbose
    }

    /// `RUST_LOG` wins; otherwise the verbosity flag sets omega's own level.
    ///
    /// Logs go to stderr, like every other thing the CLI says about itself,
    /// and the module path is left out until someone asks for it — `omega
    /// daemon` is something a person watches in a terminal, and
    /// `omega_daemon::supervisor::process` in front of every line is for
    /// whoever is debugging omega, not for whoever is running it.
    pub fn init_tracing(verbose: u8) {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            let level = match verbose {
                0 => "info",
                1 => "debug",
                _ => "trace",
            };
            EnvFilter::new(format!("omega={level}"))
        });

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .with_ansi(std::io::stderr().is_terminal())
            .with_target(verbose > 0)
            .compact()
            .init();
    }

    pub async fn dispatch(self, ui: &mut Ui) -> anyhow::Result<()> {
        self.command.dispatch(ui).await
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Build(build::BuildCmd),
    Check(check::CheckCmd),
    Clean(clean::CleanCmd),
    Daemon(daemon::DaemonCmd),
    Dev(dev::DevCmd),
    Init(init::InitCmd),
    Link(link::LinkCmd),
    Logs(logs::LogsCmd),
    New(new::NewCmd),
    Restart(restart::RestartCmd),
    Rollback(rollback::RollbackCmd),
    Run(run::RunCmd),
    Shell(shell::ShellCmd),
    Status(status::StatusCmd),
}

impl Command {
    pub async fn dispatch(self, ui: &mut Ui) -> anyhow::Result<()> {
        match self {
            Self::Build(cmd) => cmd.run(ui).await,
            Self::Check(cmd) => cmd.run(ui).await,
            Self::Clean(cmd) => cmd.run(ui),
            Self::Daemon(cmd) => cmd.run(ui).await,
            Self::Dev(cmd) => cmd.run(ui).await,
            Self::Init(cmd) => cmd.run(ui),
            Self::Link(cmd) => cmd.run(ui).await,
            Self::Logs(cmd) => cmd.run(ui).await,
            Self::New(cmd) => cmd.run(ui),
            Self::Restart(cmd) => cmd.run(ui).await,
            Self::Rollback(cmd) => cmd.run(ui),
            Self::Run(cmd) => cmd.run(ui).await,
            Self::Shell(cmd) => cmd.run(ui).await,
            Self::Status(cmd) => cmd.run(ui).await,
        }
    }
}

#[cfg(test)]
mod template_tests {
    use super::*;

    #[test]
    fn new_defaults_to_minimal_and_accepts_named_templates() {
        use crate::scaffold::Template;
        use clap::Parser;
        for (args, battery) in [
            (vec!["omega", "new", "hello"], false),
            (
                vec!["omega", "new", "hello", "--template", "minimal"],
                false,
            ),
            (vec!["omega", "new", "hello", "--template", "battery"], true),
        ] {
            let cli = Cli::try_parse_from(args).unwrap();
            let Command::New(new) = cli.command else {
                panic!("expected new");
            };
            assert_eq!(matches!(new.template, Template::Battery), battery);
        }
        let error =
            Cli::try_parse_from(["omega", "new", "hello", "--template", "unknown"]).unwrap_err();
        assert_eq!(error.kind(), clap::error::ErrorKind::InvalidValue);
    }
}
