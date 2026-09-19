mod build;
mod check;
mod clean;
mod commands;
pub(crate) mod daemon;
mod dev;
mod init;
pub(crate) mod link;
mod logs;
mod new;
mod present;
mod preview;
mod recovery;
mod restart;
mod rollback;
mod run;
pub(crate) mod shell;
mod status;
mod storage;

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

    /// `RUST_LOG` overrides verbosity flags. Write diagnostics to stderr;
    /// include module paths only at debug verbosity.
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
    Commands(commands::CommandsCmd),
    Daemon(daemon::DaemonCmd),
    Dev(dev::DevCmd),
    Init(init::InitCmd),
    Link(link::LinkCmd),
    Logs(logs::LogsCmd),
    New(new::NewCmd),
    Restart(restart::RestartCmd),
    Recovery(recovery::RecoveryCmd),
    Rollback(rollback::RollbackCmd),
    Run(run::RunCmd),
    Present(present::PresentCmd),
    Preview(preview::PreviewCmd),
    Shell(shell::ShellCmd),
    Status(status::StatusCmd),
    Storage(storage::StorageCmd),
}

impl Command {
    pub async fn dispatch(self, ui: &mut Ui) -> anyhow::Result<()> {
        match self {
            Self::Build(cmd) => cmd.run(ui).await,
            Self::Check(cmd) => cmd.run(ui).await,
            Self::Commands(cmd) => cmd.run(ui).await,
            Self::Clean(cmd) => cmd.run(ui),
            Self::Daemon(cmd) => cmd.run(ui).await,
            Self::Dev(cmd) => cmd.run(ui).await,
            Self::Init(cmd) => cmd.run(ui).await,
            Self::Link(cmd) => cmd.run(ui).await,
            Self::Logs(cmd) => cmd.run(ui).await,
            Self::New(cmd) => cmd.run(ui),
            Self::Restart(cmd) => cmd.run(ui).await,
            Self::Recovery(cmd) => cmd.run(ui),
            Self::Rollback(cmd) => cmd.run(ui),
            Self::Run(cmd) => cmd.run(ui).await,
            Self::Present(cmd) => cmd.run(ui).await,
            Self::Preview(cmd) => cmd.run(ui).await,
            Self::Shell(cmd) => cmd.run(ui).await,
            Self::Status(cmd) => cmd.run(ui).await,
            Self::Storage(cmd) => cmd.run(ui).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_validated_before_dispatch() {
        for (args, argument) in [
            (vec!["omega", "dev", "Bad"], "PLUGIN"),
            (vec!["omega", "restart", "../audio"], "PLUGIN"),
            (vec!["omega", "logs", "Bad"], "PLUGIN"),
            (vec!["omega", "status", "Bad"], "PLUGIN"),
            (vec!["omega", "run", "Bad"], "COMMAND"),
            (vec!["omega", "present", "Bad", "panel"], "PLUGIN"),
            (vec!["omega", "present", "audio", "Bad"], "SURFACE"),
            (vec!["omega", "new", "Bad"], "NAME"),
            (vec!["omega", "new", "type", "--lib"], "NAME"),
            (vec!["omega", "recovery", "inspect", "../bad"], "ID"),
            (vec!["omega", "recovery", "accept", ".."], "ID"),
            (vec!["omega", "recovery", "restore", "bad/id"], "ID"),
            (vec!["omega", "preview", "example", "--case", "Bad"], "CASE"),
        ] {
            let error = Cli::try_parse_from(args).unwrap_err();
            assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
            assert!(error.to_string().contains(argument), "{error}");
        }
    }

    #[test]
    fn typed_identifiers_preserve_command_syntax() {
        for args in [
            vec!["omega", "dev", "audio-output"],
            vec!["omega", "restart", "audio-output"],
            vec!["omega", "logs"],
            vec!["omega", "logs", "audio-output", "--follow"],
            vec!["omega", "status", "--versions"],
            vec!["omega", "status", "audio-output", "--json"],
            vec!["omega", "run", "audio-output", "set_volume", "40%"],
            vec![
                "omega",
                "present",
                "audio-output",
                "volume-panel",
                "--overlay",
            ],
            vec!["omega", "new", "audio-output"],
            vec!["omega", "new", "shared-ui", "--lib", "--into", "system"],
            vec!["omega", "recovery", "inspect", "change-1"],
            vec!["omega", "recovery", "accept", "change-1"],
            vec!["omega", "recovery", "restore", "change-1"],
            vec![
                "omega",
                "preview",
                "example",
                "--case",
                "empty-state",
                "--capture",
                "capture.png",
            ],
        ] {
            Cli::try_parse_from(args).unwrap();
        }
    }

    #[test]
    fn preview_capture_still_requires_a_case() {
        let error =
            Cli::try_parse_from(["omega", "preview", "example", "--capture", "capture.png"])
                .unwrap_err();
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
        assert!(error.to_string().contains("--case"));
    }

    #[test]
    fn link_accepts_native_paths_without_lossy_conversion() {
        use std::os::unix::ffi::OsStringExt;
        let path = std::ffi::OsString::from_vec(b"/tmp/omega-\xff".to_vec());
        let cli = Cli::try_parse_from([
            std::ffi::OsString::from("omega"),
            "link".into(),
            path.clone(),
        ])
        .unwrap();
        let Command::Link(link) = cli.command else {
            panic!("expected link")
        };
        assert_eq!(link.path.unwrap().into_os_string(), path);
    }

    #[test]
    fn new_command_host_accepts_only_its_own_options() {
        let cli =
            Cli::try_parse_from(["omega", "new", "audio-commands", "--command-host"]).unwrap();
        let Command::New(new) = cli.command else {
            panic!("expected new")
        };
        assert!(new.command_host);
        for extra in [
            vec!["--lib"],
            vec!["--template", "minimal"],
            vec!["--into", "system"],
        ] {
            let mut args = vec!["omega", "new", "audio", "--command-host"];
            args.extend(extra);
            assert!(Cli::try_parse_from(args).is_err());
        }
        assert!(
            Cli::try_parse_from([
                "omega",
                "new",
                "shared",
                "--lib",
                "--into",
                "commands/audio"
            ])
            .is_ok()
        );
    }

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
