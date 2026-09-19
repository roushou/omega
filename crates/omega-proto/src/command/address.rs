use super::CommandContractError;
use crate::omega::CommandDependency;
use crate::{CommandId, PluginName};

/// A resolved provider and operation used for routing and diagnostics.
/// Command identity and signatures depend only on `command`.
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
impl TryFrom<&CommandDependency> for CommandId {
    type Error = CommandContractError;
    fn try_from(value: &CommandDependency) -> Result<Self, Self::Error> {
        Ok(value.command.parse()?)
    }
}
