//! Resolve command authority and target session together before forwarding.
use super::PluginRegistry;
use crate::{
    authorization::Grants,
    refusal::{Refusable, RefusableResult},
};
use omega_proto::omega::{
    AvailableCommand, CallCommand, CommandCatalogue, InvokePlugin, command_type::Kind, invoke,
};
use omega_proto::{CommandAddress, CommandAnswer, Refusal};

impl PluginRegistry {
    pub(crate) async fn invoke_command(
        &self,
        call: &InvokePlugin,
        grants: Option<&Grants>,
    ) -> Result<CommandAnswer, Refusal> {
        let address = CommandAddress {
            plugin: call
                .plugin
                .parse()
                .map_err(|e: omega_proto::IdentError| Refusal::invalid(e.to_string()))?,
            command: call
                .command
                .parse()
                .map_err(|e: omega_proto::IdentError| Refusal::invalid(e.to_string()))?,
        };
        if let Some(grants) = grants {
            grants.command_access(&address)?;
        }
        let (session, endpoint) = {
            let records = self.lock();
            let record = records.get(&address.plugin).ok_or_else(|| {
                Refusal::invalid(format!("{} is not a plugin of this build", address.plugin))
            })?;
            let manifest = record
                .session
                .as_ref()
                .and_then(|s| s.manifest.as_deref())
                .or_else(|| record.manifest.as_ref().map(|m| &m.manifest))
                .ok_or_else(|| Refusal::precondition("plugin has no manifest"))?;
            let endpoint = manifest
                .commands
                .iter()
                .find(|endpoint| endpoint.id == address.command.as_str())
                .ok_or_else(|| {
                    Refusal::invalid(format!(
                        "undeclared command {address}; available: {}",
                        manifest
                            .commands
                            .iter()
                            .map(|c| c.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                })?;
            let signature = endpoint.signature(address.plugin.as_str());
            if let Some(grants) = grants {
                grants.command(&address, &signature)?;
            }
            if !call.signature.is_empty() && signature != call.signature {
                return Err(Refusal::precondition(format!(
                    "incompatible command {address}"
                )));
            }
            if !call.signature.is_empty() {
                let input = endpoint
                    .input
                    .as_ref()
                    .ok_or_else(|| Refusal::precondition("missing command input contract"))?;
                match Kind::try_from(input.kind)
                    .map_err(|_| Refusal::precondition("unknown command input shape"))?
                {
                    Kind::Opaque => {}
                    Kind::Unit if call.args.is_empty() => {}
                    Kind::Unit => return Err(Refusal::invalid("command expects no arguments")),
                    _ if call.args.len() == 1 => {
                        input.accepts(&call.args[0]).map_err(|e| e.refusal())?
                    }
                    _ => return Err(Refusal::invalid("command expects one argument")),
                }
            }
            let session = record.session.as_ref().ok_or_else(|| {
                Refusal::unavailable(format!("{} is not connected", address.plugin))
            })?;
            (session.clone(), endpoint.clone())
        };
        let answer = CommandAnswer::try_from(
            Self::request_on(
                &session,
                &address.plugin,
                invoke::Op::CallCommand(CallCommand {
                    command: address.command.to_string(),
                    args: call.args.clone(),
                }),
            )
            .await
            .or_refuse()?,
        )?;
        if let Some(output) = &endpoint.output {
            let value = match &answer {
                CommandAnswer::Value(value) => value,
                CommandAnswer::Acknowledged
                    if matches!(Kind::try_from(output.kind), Ok(Kind::Unit | Kind::Opaque)) =>
                {
                    return Ok(answer);
                }
                _ => return Err(Refusal::precondition("command returned no value")),
            };
            output
                .accepts(value)
                .map_err(|_| Refusal::precondition(format!("malformed result from {address}")))?;
        }
        Ok(answer)
    }

    pub(crate) fn command_catalogue(
        &self,
        grants: Option<&Grants>,
    ) -> Result<CommandCatalogue, Refusal> {
        let records = self.lock();
        let mut entries = Vec::new();
        for record in records.values() {
            let manifest = record
                .session
                .as_ref()
                .and_then(|session| session.manifest.as_deref())
                .or_else(|| record.manifest.as_ref().map(|m| &m.manifest));
            let Some(manifest) = manifest else {
                continue;
            };
            for endpoint in &manifest.commands {
                let command = endpoint.validate().map_err(|e| e.refusal())?;
                let address = CommandAddress {
                    plugin: record.name.clone(),
                    command,
                };
                let signature = endpoint.signature(record.name.as_str());
                if grants.is_some_and(|grants| grants.command(&address, &signature).is_err()) {
                    continue;
                }
                entries.push(AvailableCommand {
                    plugin: record.name.to_string(),
                    endpoint: Some(endpoint.clone()),
                    signature,
                    available: record.session.is_some(),
                });
            }
        }
        entries.sort_by(|a, b| {
            (&a.plugin, a.endpoint.as_ref().map(|e| &e.id))
                .cmp(&(&b.plugin, b.endpoint.as_ref().map(|e| &e.id)))
        });
        Ok(CommandCatalogue { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hub::Hub, manifest::ManifestStore};
    use omega_proto::{
        IntoValue, Manifest,
        omega::{CommandEndpoint, CommandType, ErrorCode, result},
    };

    struct Fixture;
    impl Fixture {
        fn manifest(output: Kind) -> Manifest {
            Manifest::new(&"target".parse().unwrap(), "1").serving([CommandEndpoint {
                id: "get".into(),
                input: Some(CommandType::of(Kind::Unit)),
                output: Some(CommandType::of(output)),
                description: String::new(),
            }])
        }
        fn call(manifest: &Manifest) -> InvokePlugin {
            InvokePlugin {
                plugin: "target".into(),
                command: "get".into(),
                args: Vec::new(),
                signature: manifest.commands[0].signature("target"),
            }
        }
    }

    #[tokio::test]
    async fn dispatch_uses_the_connected_contract_until_a_new_session_replaces_it() {
        let old = Fixture::manifest(Kind::Text);
        let new = Fixture::manifest(Kind::Boolean);
        let registry = PluginRegistry::detached(Hub::new());
        registry.adopt(&ManifestStore::from_manifests([old.clone()]));
        let (outbound, mut old_calls) = tokio::sync::mpsc::channel(16);
        let old_guard = registry.connected(&"target".parse().unwrap(), outbound);
        registry.adopt(&ManifestStore::from_manifests([new.clone()]));

        let dispatch = registry.clone();
        let call = Fixture::call(&old);
        let task = tokio::spawn(async move { dispatch.invoke_command(&call, None).await });
        let admitted = old_calls.recv().await.unwrap();
        let (outbound, mut new_calls) = tokio::sync::mpsc::channel(16);
        let _new_guard = registry.connected(&"target".parse().unwrap(), outbound);
        drop(old_guard);
        admitted
            .answer
            .send(Ok(result::Outcome::Value("old".into_value())))
            .unwrap();
        assert_eq!(
            task.await.unwrap().unwrap(),
            CommandAnswer::Value("old".into_value())
        );
        assert!(
            new_calls.try_recv().is_err(),
            "admitted work is never replayed"
        );
        assert_eq!(
            registry
                .invoke_command(&Fixture::call(&old), None)
                .await
                .unwrap_err()
                .code,
            ErrorCode::FailedPrecondition
        );
        assert!(
            new_calls.try_recv().is_err(),
            "incompatible calls never execute"
        );

        let dispatch = registry.clone();
        let call = Fixture::call(&new);
        let task = tokio::spawn(async move { dispatch.invoke_command(&call, None).await });
        new_calls
            .recv()
            .await
            .unwrap()
            .answer
            .send(Ok(result::Outcome::Value(true.into_value())))
            .unwrap();
        assert_eq!(
            task.await.unwrap().unwrap(),
            CommandAnswer::Value(true.into_value())
        );
    }

    #[tokio::test]
    async fn malformed_results_are_rejected_against_the_admitted_contract() {
        let manifest = Fixture::manifest(Kind::Text);
        let registry = PluginRegistry::detached(Hub::new());
        registry.adopt(&ManifestStore::from_manifests([manifest.clone()]));
        let (outbound, mut calls) = tokio::sync::mpsc::channel(16);
        let _guard = registry.connected(&"target".parse().unwrap(), outbound);
        let dispatch = registry.clone();
        let call = Fixture::call(&manifest);
        let task = tokio::spawn(async move { dispatch.invoke_command(&call, None).await });
        calls
            .recv()
            .await
            .unwrap()
            .answer
            .send(Ok(result::Outcome::Value(false.into_value())))
            .unwrap();
        assert_eq!(
            task.await.unwrap().unwrap_err().code,
            ErrorCode::FailedPrecondition
        );
    }
}
