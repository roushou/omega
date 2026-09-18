use super::CommandContractError;
use crate::omega::CommandDependency;
use crate::{CommandId, PluginName};

/// A command's complete identity. Neither component is inferred from a caller.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommandAddress {
    pub plugin: PluginName,
    pub command: CommandId,
}
impl std::fmt::Display for CommandAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.plugin, self.command)
    }
}
impl TryFrom<&CommandDependency> for CommandAddress {
    type Error = CommandContractError;
    fn try_from(value: &CommandDependency) -> Result<Self, Self::Error> {
        Ok(Self {
            plugin: value.plugin.parse()?,
            command: value.command.parse()?,
        })
    }
}
