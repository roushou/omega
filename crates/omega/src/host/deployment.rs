use super::{CommandHost, ExecutionPolicy, HostPolicy, Lifetime, StartPolicy};
use omega_proto::{Values, omega::CommandHostConfig};

/// Desired command-host lifetime, execution bounds, and construction settings.
///
/// Defaults to persistent on-demand execution, serial calls, a queue of 32,
/// and five-second queue, startup, and execution deadlines. Add this deployment
/// to the system document; constructing it starts no process. The daemon applies
/// these settings when it launches the declared executable.
///
/// ```
/// use omega::host::{CommandHost, StartPolicy};
/// # #[derive(omega::Command)]
/// # struct Echo;
/// # impl omega::Command for Echo {
/// #     type Input = (); type Output = (); const ID: &'static str = "example.echo";
/// #     async fn call(&self, _: ()) -> omega::Result<()> { Ok(()) }
/// # }
/// let deployment = CommandHost::new("audio", "1.0.0")
///     .command::<Echo>()
///     .deployment()
///     .persistent(StartPolicy::Eager);
/// let configuration = deployment.configuration()?;
/// assert_eq!(configuration.id, "audio");
/// # Ok::<(), omega::Error>(())
/// ```
#[derive(Debug)]
pub struct CommandHostDeployment {
    declaration: CommandHost,
    lifetime: Lifetime,
    execution: ExecutionPolicy,
    settings: Values,
}

impl From<CommandHost> for CommandHostDeployment {
    fn from(declaration: CommandHost) -> Self {
        Self {
            declaration,
            lifetime: Lifetime::Persistent(StartPolicy::OnDemand),
            execution: ExecutionPolicy::default(),
            settings: Values::new(),
        }
    }
}

impl CommandHostDeployment {
    /// Keep one process running; select startup on the first call or at activation.
    pub fn persistent(mut self, start: StartPolicy) -> Self {
        self.lifetime = Lifetime::Persistent(start);
        self
    }

    /// Start a fresh process for each admitted invocation and exit after its terminal answer.
    pub fn one_shot(mut self) -> Self {
        self.lifetime = Lifetime::OneShot;
        self
    }

    /// Set admission bounds and phase deadlines. Invalid limits fail configuration validation.
    pub fn execution(mut self, policy: ExecutionPolicy) -> Self {
        self.execution = policy;
        self
    }

    /// Supply provider-wide construction settings. Changing them restarts the host.
    pub fn settings(mut self, settings: impl omega_proto::Fields) -> Self {
        self.settings = settings.write();
        self
    }

    /// Validate the declaration and execution limits, then produce desired configuration.
    /// Invalid identities, registrations, or policy values return an error. No process starts.
    pub fn configuration(&self) -> crate::Result<CommandHostConfig> {
        let manifest = self.declaration.manifest()?;
        let policy = HostPolicy {
            id: manifest
                .name
                .parse()
                .map_err(|error: omega_proto::IdentError| {
                    crate::Error::invalid(error.to_string())
                })?,
            lifetime: self.lifetime,
            execution: self.execution.clone(),
        };
        let mut configuration: CommandHostConfig =
            policy
                .try_into()
                .map_err(|error: omega_proto::host::HostPolicyError| {
                    crate::Error::invalid(error.to_string())
                })?;
        configuration.settings = self.settings.clone().into_map();
        Ok(configuration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(crate::Command)]
    struct Echo;
    impl crate::Command for Echo {
        type Input = ();
        type Output = ();
        const ID: &'static str = "test.echo";
        async fn call(&self, _: ()) -> crate::Result<()> {
            Ok(())
        }
    }

    #[derive(crate::Config, Default)]
    struct Settings {
        enabled: bool,
    }

    struct Fixture;
    impl Fixture {
        fn host() -> CommandHost {
            CommandHost::new("example", "1").command::<Echo>()
        }
    }

    #[test]
    fn defaults_are_identical_through_explicit_and_implicit_conversion() {
        let implicit: CommandHostDeployment = Fixture::host().into();
        let explicit = Fixture::host()
            .deployment()
            .persistent(StartPolicy::OnDemand)
            .execution(ExecutionPolicy::serial());
        let config = implicit.configuration().unwrap();
        assert_eq!(config, explicit.configuration().unwrap());
        assert_eq!(
            HostPolicy::try_from(&config).unwrap().lifetime,
            Lifetime::Persistent(StartPolicy::OnDemand)
        );
    }

    #[test]
    fn deployment_settings_and_policy_are_validated_without_changing_exports() {
        let deployment = Fixture::host()
            .deployment()
            .one_shot()
            .settings(Settings { enabled: true });
        let config = deployment.configuration().unwrap();
        assert_eq!(
            HostPolicy::try_from(&config).unwrap().lifetime,
            Lifetime::OneShot
        );
        assert!(!config.settings.is_empty());
        assert_eq!(
            deployment.declaration.manifest().unwrap(),
            Fixture::host().manifest().unwrap()
        );
        let mut invalid = ExecutionPolicy::serial();
        invalid.queue_timeout = std::time::Duration::ZERO;
        assert!(
            Fixture::host()
                .deployment()
                .execution(invalid)
                .configuration()
                .is_err()
        );
        assert!(
            CommandHost::new("bad name", "1")
                .command::<Echo>()
                .deployment()
                .configuration()
                .is_err()
        );
        assert!(
            CommandHost::new("empty", "1")
                .deployment()
                .configuration()
                .is_err()
        );
    }
}
