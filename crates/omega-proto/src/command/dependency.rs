use super::{CommandAddress, CommandContractError};
use crate::omega::{CommandDependency, CommandEndpoint, Manifest};
use std::collections::BTreeMap;

impl CommandEndpoint {
    pub fn dependency(&self, plugin: &str) -> CommandDependency {
        CommandDependency {
            plugin: plugin.into(),
            command: self.id.clone(),
            signature: self.signature(plugin),
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
                let address = CommandAddress {
                    plugin: manifest.name.parse()?,
                    command: endpoint.validate()?,
                };
                if endpoints
                    .insert(address.clone(), endpoint.signature(&manifest.name))
                    .is_some()
                {
                    return Err(CommandContractError::Invalid(format!(
                        "duplicate {address}"
                    )));
                }
            }
        }
        for manifest in manifests {
            for dependency in &manifest.command_dependencies {
                let address = CommandAddress::try_from(dependency)?;
                if endpoints.get(&address) != Some(&dependency.signature) {
                    return Err(CommandContractError::Invalid(format!(
                        "{} requires absent or incompatible {address}",
                        manifest.name
                    )));
                }
            }
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
    fn dependencies_require_an_existing_compatible_endpoint() {
        let endpoint = Fixture::endpoint();
        let target = Manifest::new(&"target".parse().unwrap(), "1").serving([endpoint.clone()]);
        let mut caller = Manifest::new(&"caller".parse().unwrap(), "1");
        caller
            .command_dependencies
            .push(endpoint.dependency("target"));
        assert!(CommandContracts::validate([&target, &caller]).is_ok());
        assert!(CommandContracts::validate([&caller]).is_err());
        caller.command_dependencies[0].signature[0] ^= 1;
        assert!(CommandContracts::validate([&target, &caller]).is_err());
    }
}
