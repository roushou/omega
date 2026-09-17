//! Reconcile built-plugin lifecycle. Unlisted built plugins are enabled; explicit
//! `enabled = false` disables a plugin without removing its artifact.

use std::collections::BTreeSet;

use omega_host::Generation;
use omega_proto::PluginName;
use omega_proto::omega::StateDocument;

use crate::reconcile::ProviderError;
use crate::supervisor::{PluginLog, PluginSpec, Supervisor};

#[derive(Debug)]
pub struct PluginProvider {
    supervisor: Supervisor,
    generation: Generation,
}

impl PluginProvider {
    pub fn new(supervisor: Supervisor, generation: Generation) -> Self {
        Self {
            supervisor,
            generation,
        }
    }

    /// Plan enabled built plugins against captured supervision ownership.
    pub fn plan(
        document: &StateDocument,
        built: &BTreeSet<PluginName>,
        running: &BTreeSet<PluginName>,
    ) -> Result<Vec<PluginChange>, ProviderError> {
        let mut desired = built.clone();
        let mut configured = BTreeSet::new();
        for plugin in &document.plugins {
            let name = plugin
                .name
                .parse::<PluginName>()
                .map_err(|error| ProviderError::new("plugins", error.to_string()))?;
            if !built.contains(&name) || !configured.insert(name.clone()) {
                return Err(ProviderError::new(
                    "plugins",
                    format!("unknown or duplicate plugin {name}"),
                ));
            }
            if !plugin.enabled {
                desired.remove(&name);
            }
        }
        let mut changes: Vec<_> = desired
            .difference(running)
            .cloned()
            .map(PluginChange::Start)
            .chain(
                running
                    .difference(&desired)
                    .cloned()
                    .map(PluginChange::Stop),
            )
            .collect();
        changes.sort_by(|a, b| a.name().cmp(b.name()));
        Ok(changes)
    }

    pub async fn apply(&self, changes: &[PluginChange]) -> Result<(), ProviderError> {
        let _handover = self.supervisor.handover().await;
        for change in changes {
            let name = change.name();
            tracing::info!(plugin = %name, ?change, "converging plugin");
            match change {
                PluginChange::Start(_) => {
                    if self.supervisor.running().contains(name) {
                        continue;
                    }
                    let spec = PluginSpec::for_generation(name.clone(), self.generation.clone());
                    self.supervisor.spawn(
                        spec.logged(PluginLog::at(self.generation.layout().plugin_log(name))),
                    );
                }
                PluginChange::Stop(_) => {
                    self.supervisor.stop(name).await;
                    if self.supervisor.running().contains(name) {
                        return Err(ProviderError::new(
                            "plugins",
                            format!("{name} has not stopped"),
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginChange {
    Start(PluginName),
    Stop(PluginName),
}

impl PluginChange {
    pub fn name(&self) -> &PluginName {
        match self {
            Self::Start(name) | Self::Stop(name) => name,
        }
    }
}
