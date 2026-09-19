//! Typed completion of isolated command calls.
use crate::{
    Command, Input,
    command::{CommandRef, CommandValue},
    testing::CapturedEffect,
};
use omega_proto::{
    IntoValue,
    omega::{action, command_type::Kind, invoke},
};

/// A captured invocation with decoded input. Complete it explicitly; no live
/// plugin is contacted. Dropping it closes the isolated completion receipt.
pub struct CommandCall<C: Command> {
    input: C::Input,
    effect: CapturedEffect,
}

impl<C: Command> std::fmt::Debug for CommandCall<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandCall")
            .field("plugin", &"")
            .field("command", &C::ID)
            .finish_non_exhaustive()
    }
}

impl<C: Command> CommandCall<C> {
    pub fn input(&self) -> &C::Input {
        &self.input
    }
    pub fn complete(self, result: crate::Result<C::Output>) -> crate::Result<()> {
        let result = match result {
            Ok(value) => {
                let value = value.into_value();
                C::Output::shape()
                    .accepts(&value)
                    .map_err(|e| crate::Error::invalid(e.to_string()))?;
                Ok(if C::Output::shape().kind == Kind::Unit as i32 {
                    None
                } else {
                    Some(value)
                })
            }
            Err(error) => Err(crate::effect::EffectError::Refused(error.refusal())),
        };
        self.effect.complete(result)?;
        Ok(())
    }
}

impl CapturedEffect {
    /// Decode a call to the exact declared endpoint, including its signature.
    pub fn command<C: Command>(self) -> crate::Result<CommandCall<C>> {
        let invoke::Op::Act(act) = self.operation() else {
            return Err(crate::Error::invalid("expected a command invocation"));
        };
        let Some(action::Kind::InvokePlugin(call)) =
            act.action.as_ref().and_then(|a| a.kind.as_ref())
        else {
            return Err(crate::Error::invalid("expected a command invocation"));
        };
        if !call.plugin.is_empty()
            || call.command != C::ID
            || call.signature != CommandRef::<C>::INSTANCE.descriptor().signature()
        {
            return Err(crate::Error::invalid(
                "captured command identity or signature differs",
            ));
        }
        let input = C::Input::decode(crate::Args::new(call.args.clone()))?;
        Ok(CommandCall {
            input,
            effect: self,
        })
    }
}

/// A pending catalogue read. Add typed endpoint declarations before completing it.
#[derive(Debug)]
pub struct CommandList {
    effect: CapturedEffect,
    catalogue: omega_proto::omega::CommandCatalogue,
}

impl CommandList {
    pub fn entry<C: Command>(mut self, command: CommandRef<C>, available: bool) -> Self {
        let descriptor = command.descriptor();
        self.catalogue
            .entries
            .push(omega_proto::omega::AvailableCommand {
                executions: Vec::new(),
                plugin: "fixture".into(),
                signature: descriptor.signature(),
                endpoint: Some(descriptor),
                available,
            });
        self
    }
    pub fn complete(self) -> crate::Result<()> {
        self.effect
            .complete(Ok(Some(self.catalogue.into_value())))?;
        Ok(())
    }
}

impl CapturedEffect {
    pub fn commands(self) -> crate::Result<CommandList> {
        if !matches!(self.operation(), invoke::Op::ListCommands(_)) {
            return Err(crate::Error::invalid("expected command catalogue read"));
        }
        Ok(CommandList {
            effect: self,
            catalogue: Default::default(),
        })
    }
}
