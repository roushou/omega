//! Parse build options and delegate to the workspace build operation.
use crate::{build::Build, ui::Ui};
use omega_host::{Layout, Profile};
use std::time::Duration;

/// Compile `~/.config/omega` and assemble `~/.local/state/omega`.
#[derive(Debug, clap::Args)]
pub struct BuildCmd {
    /// Compile using the debug profile without optimizations.
    #[arg(long)]
    pub debug: bool,

    /// Wait for this build to be accepted, reconciled, and its shell applied.
    #[arg(long)]
    pub wait: bool,

    /// Maximum activation wait (seconds, optionally followed by s). Defaults to 30s.
    #[arg(long, requires = "wait", value_parser = Self::parse_timeout)]
    pub timeout: Option<Duration>,
}

impl BuildCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        Build {
            layout: Layout::resolve(),
            profile: if self.debug {
                Profile::Debug
            } else {
                Profile::Release
            },
            activation_timeout: self
                .wait
                .then_some(self.timeout.unwrap_or(Duration::from_secs(30))),
        }
        .run(ui)
        .await
    }
    fn parse_timeout(input: &str) -> Result<Duration, String> {
        let seconds: u64 = input
            .strip_suffix('s')
            .unwrap_or(input)
            .parse()
            .map_err(|_| "expected a positive number of seconds, such as 30s".to_string())?;
        if seconds == 0 || seconds > 86400 {
            return Err("timeout must be between 1s and 86400s".into());
        }
        Ok(Duration::from_secs(seconds))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timeout_arguments_are_explicit_and_bounded() {
        use clap::Parser;
        for args in [
            vec!["omega", "build", "--timeout", "3s"],
            vec!["omega", "build", "--wait", "--timeout", "0"],
            vec!["omega", "build", "--wait", "--timeout", "999999999999"],
            vec!["omega", "status", "--json", "--versions"],
        ] {
            assert!(crate::cli::Cli::try_parse_from(args).is_err());
        }
        assert_eq!(
            BuildCmd::parse_timeout("30s").unwrap(),
            Duration::from_secs(30)
        );
    }
}
