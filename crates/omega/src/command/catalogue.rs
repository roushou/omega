//! Inspect commands already available to this plugin's declared dependencies.
use crate::{
    effect::{Effect, EffectError},
    runtime::context::Context,
    wiring::{Does, Wiring},
};
use omega_proto::{
    CommandAddress, FromValue,
    omega::{
        Act, Action, CommandCatalogue, CommandEndpoint, InvokePlugin, ListCommands, Value, action,
        invoke,
    },
};

/// One discoverable endpoint and its current provider availability.
/// Configured on-demand hosts can be available before their process starts.
#[derive(Debug, Clone)]
pub struct Available {
    pub address: CommandAddress,
    pub descriptor: CommandEndpoint,
    pub available: bool,
    signature: Vec<u8>,
}
/// Read the caller-scoped command catalogue and invoke dynamically selected entries.
/// Listing never grants access: declare each target using [`super::Caller`].
/// The daemon rechecks the signature and access when an entry is invoked.
#[derive(Debug, Clone)]
pub struct Commands {
    context: Context,
}
impl Wiring for Commands {
    fn build(context: &Context) -> Self {
        Self {
            context: context.clone(),
        }
    }
}
impl Does for Commands {}
impl Commands {
    /// Return a sorted snapshot. Availability may change before a subsequent call.
    pub fn list(&self) -> Effect<Vec<Available>> {
        Effect::decoded(
            self.context.act(invoke::Op::ListCommands(ListCommands {})),
            |value| {
                let catalogue =
                    CommandCatalogue::from_value(&value.ok_or(EffectError::UnexpectedResponse)?)
                        .ok_or(EffectError::UnexpectedResponse)?;
                catalogue
                    .entries
                    .into_iter()
                    .map(|entry| {
                        let descriptor = entry.endpoint.ok_or(EffectError::UnexpectedResponse)?;
                        let address = CommandAddress {
                            plugin: entry
                                .plugin
                                .parse()
                                .map_err(|_| EffectError::UnexpectedResponse)?,
                            command: descriptor
                                .validate()
                                .map_err(|_| EffectError::UnexpectedResponse)?,
                        };
                        if entry.signature != descriptor.signature() {
                            return Err(EffectError::UnexpectedResponse);
                        }
                        Ok(Available {
                            address,
                            descriptor,
                            available: entry.available,
                            signature: entry.signature,
                        })
                    })
                    .collect()
            },
        )
    }
    /// Invoke a discovered command with its wire arguments. Prefer [`super::Caller`]
    /// for statically known endpoints. Results retain their value or acknowledgement.
    pub fn invoke(&self, command: &Available, args: super::Args) -> Effect<Option<Value>> {
        let call = InvokePlugin {
            plugin: command.address.plugin.to_string(),
            command: command.address.command.to_string(),
            signature: command.signature.clone(),
            args: args.into_values(),
        };
        Effect::decoded(
            self.context.act(invoke::Op::Act(Act {
                action: Some(Action {
                    kind: Some(action::Kind::InvokePlugin(call)),
                }),
            })),
            Ok,
        )
    }
}
