use super::CommandContractError;
use crate::omega::{CommandDependency, CommandEndpoint, Manifest};
use std::collections::BTreeMap;

impl CommandEndpoint {
    pub fn dependency(&self) -> CommandDependency {
        CommandDependency {
            command: self.id.clone(),
            signature: self.signature(),
        }
    }
}
/// Cross-check declarations once all workspace manifests are available.
#[derive(Debug)]
pub struct CommandContracts;
impl CommandContracts {
    pub fn validate<'a>(
        manifests: impl IntoIterator<Item = &'a Manifest>,
    ) -> Result<(), CommandContractError> {
        let manifests: Vec<_> = manifests.into_iter().collect();
        let mut endpoints = BTreeMap::new();
        for manifest in &manifests {
            for endpoint in &manifest.commands {
                let address = endpoint.validate()?;
                if endpoints
                    .insert(address.clone(), endpoint.signature())
                    .is_some()
                {
                    return Err(CommandContractError::Invalid(format!(
                        "duplicate {address}"
                    )));
                }
            }
        }
        let mut edges = BTreeMap::<crate::PluginName, Vec<crate::PluginName>>::new();
        for manifest in &manifests {
            for dependency in &manifest.command_dependencies {
                let address = crate::CommandId::try_from(dependency)?;
                {
                    let target = manifests
                        .iter()
                        .find(|target| {
                            target
                                .commands
                                .iter()
                                .any(|endpoint| endpoint.id == address.as_str())
                        })
                        .ok_or_else(|| {
                            CommandContractError::Invalid(format!("missing provider for {address}"))
                        })?;
                    edges
                        .entry(
                            manifest
                                .plugin()
                                .map_err(|e| CommandContractError::Invalid(e.to_string()))?,
                        )
                        .or_default()
                        .push(
                            target
                                .plugin()
                                .map_err(|e| CommandContractError::Invalid(e.to_string()))?,
                        );
                }
                if endpoints.get(&address) != Some(&dependency.signature) {
                    return Err(CommandContractError::Invalid(format!(
                        "{} requires absent or incompatible {address}",
                        manifest.name
                    )));
                }
            }
        }
        let identities = manifests
            .iter()
            .map(|manifest| {
                manifest
                    .plugin()
                    .map_err(|error| CommandContractError::Invalid(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut resolved = std::collections::BTreeSet::new();
        loop {
            let ready: Vec<_> = identities
                .iter()
                .filter(|id| {
                    !resolved.contains(*id)
                        && edges.get(*id).is_none_or(|targets| {
                            targets.iter().all(|target| resolved.contains(target))
                        })
                })
                .cloned()
                .collect();
            if ready.is_empty() {
                break;
            }
            resolved.extend(ready);
        }
        if let Some(host) = edges.keys().find(|host| !resolved.contains(*host)) {
            return Err(CommandContractError::Invalid(format!(
                "command provider dependency cycle at {host}"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::omega::{CommandType, command_type::Kind};
    struct Fixture;
    impl Fixture {
        fn endpoint() -> CommandEndpoint {
            CommandEndpoint {
                id: "set".into(),
                input: Some(CommandType::of(Kind::Text)),
                output: Some(CommandType::of(Kind::Unit)),
                description: "Set a value".into(),
            }
        }
    }
    #[test]
    fn providers_are_unique_and_cycles_include_plugin_hosts() {
        let a = Fixture::endpoint();
        let mut b = Fixture::endpoint();
        b.id = "second".into();
        let mut first = Manifest::new(&"first".parse().unwrap(), "1").serving([a.clone()]);
        let mut second = Manifest::new(&"second".parse().unwrap(), "1").serving([b.clone()]);
        first.command_dependencies.push(b.dependency());
        assert!(CommandContracts::validate([&first, &second]).is_ok());
        second.command_dependencies.push(a.dependency());
        assert!(
            CommandContracts::validate([&first, &second])
                .unwrap_err()
                .to_string()
                .contains("cycle")
        );
        second.command_dependencies.clear();
        second.commands = vec![a];
        assert!(
            CommandContracts::validate([&first, &second])
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
    }

    #[test]
    fn dependencies_require_an_existing_compatible_endpoint() {
        let endpoint = Fixture::endpoint();
        let target = Manifest::new(&"target".parse().unwrap(), "1").serving([endpoint.clone()]);
        let mut caller = Manifest::new(&"caller".parse().unwrap(), "1");
        caller.command_dependencies.push(endpoint.dependency());
        assert!(CommandContracts::validate([&target, &caller]).is_ok());
        assert!(CommandContracts::validate([&caller]).is_err());
        caller.command_dependencies[0].signature[0] ^= 1;
        assert!(CommandContracts::validate([&target, &caller]).is_err());
    }
}
