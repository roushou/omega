//! Which units run.
//!
//! Two planes meet here: the workspace decides what is *built*, the document
//! decides what is *enabled*. A built unit the document never mentions runs —
//! putting a crate in the workspace is already a declaration — and naming it
//! with `enabled = false` turns it off without deleting it.

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
    /// Every unit the build produced.
    built: BTreeSet<UnitName>,
}

impl UnitProvider {
    pub fn new(
        supervisor: Supervisor,
        generation: Generation,
        built: impl IntoIterator<Item = UnitName>,
    ) -> Self {
        Self {
            supervisor,
            generation,
            built: built.into_iter().collect(),
        }
    }

    pub fn plan(&self, document: &StateDocument) -> Result<Vec<UnitChange>, ProviderError> {
        let mut desired = self.built.clone();
        let mut configured = BTreeSet::new();
        for unit in &document.units {
            let name = UnitName::parse(&unit.name)
                .map_err(|error| ProviderError::new("units", error.to_string()))?;
            if !self.built.contains(&name) || !configured.insert(name.clone()) {
                return Err(ProviderError::new(
                    "units",
                    format!("unknown or duplicate unit {name}"),
                ));
            }
            if !unit.enabled {
                desired.remove(&name);
            }
        }
        let running: BTreeSet<_> = self.supervisor.running().into_iter().collect();
        let mut changes: Vec<_> = desired
            .difference(&running)
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
