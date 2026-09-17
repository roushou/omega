//! Reconcile built-unit lifecycle. Unlisted built units are enabled; explicit
//! `enabled = false` disables a unit without removing its artifact.

use std::collections::BTreeSet;

use omega_host::Generation;
use omega_proto::UnitName;
use omega_proto::omega::StateDocument;

use crate::reconcile::ProviderError;
use crate::supervisor::{Supervisor, UnitLog, UnitSpec};

#[derive(Debug)]
pub struct UnitProvider {
    supervisor: Supervisor,
    generation: Generation,
}

impl UnitProvider {
    pub fn new(supervisor: Supervisor, generation: Generation) -> Self {
        Self {
            supervisor,
            generation,
        }
    }

    /// Plan enabled built units against captured supervision ownership.
    pub fn plan(
        document: &StateDocument,
        built: &BTreeSet<UnitName>,
        running: &BTreeSet<UnitName>,
    ) -> Result<Vec<UnitChange>, ProviderError> {
        let mut desired = built.clone();
        let mut configured = BTreeSet::new();
        for unit in &document.units {
            let name = unit
                .name
                .parse::<UnitName>()
                .map_err(|error| ProviderError::new("units", error.to_string()))?;
            if !built.contains(&name) || !configured.insert(name.clone()) {
                return Err(ProviderError::new(
                    "units",
                    format!("unknown or duplicate unit {name}"),
                ));
            }
            if !unit.enabled {
                desired.remove(&name);
            }
        }
        let mut changes: Vec<_> = desired
            .difference(running)
            .cloned()
            .map(UnitChange::Start)
            .chain(running.difference(&desired).cloned().map(UnitChange::Stop))
            .collect();
        changes.sort_by(|a, b| a.name().cmp(b.name()));
        Ok(changes)
    }

    pub async fn apply(&self, changes: &[UnitChange]) -> Result<(), ProviderError> {
        let _handover = self.supervisor.handover().await;
        for change in changes {
            let name = change.name();
            tracing::info!(unit = %name, ?change, "converging unit");
            match change {
                UnitChange::Start(_) => {
                    if self.supervisor.running().contains(name) {
                        continue;
                    }
                    let spec = UnitSpec::for_generation(name.clone(), self.generation.clone());
                    self.supervisor
                        .spawn(spec.logged(UnitLog::at(self.generation.layout().unit_log(name))));
                }
                UnitChange::Stop(_) => {
                    self.supervisor.stop(name).await;
                    if self.supervisor.running().contains(name) {
                        return Err(ProviderError::new(
                            "units",
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
pub enum UnitChange {
    Start(UnitName),
    Stop(UnitName),
}

impl UnitChange {
    pub fn name(&self) -> &UnitName {
        match self {
            Self::Start(name) | Self::Stop(name) => name,
        }
    }
}
