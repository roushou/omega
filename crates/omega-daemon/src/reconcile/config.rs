//! What each unit was configured with.
//!
//! A unit's settings are construction, not state: a plugin's fields are built
//! out of them, so a unit given different settings has to be built again —
//! and the honest way to build a plugin again is to run it again. This is the
//! provider that records what the document says and cycles whatever that
//! changed.
//!
//! It runs before the provider that starts units, because a unit must not be
//! spawned before the settings its own handshake will carry are on file.

use std::collections::HashMap;

use async_trait::async_trait;

use omega_core::UnitName;
use omega_wire::omega::{StateDocument, Value};

use crate::reconcile::{Change, Provider, ProviderError};
use crate::supervisor::Supervisor;
use crate::units::UnitTable;

#[derive(Debug)]
pub struct ConfigProvider {
    units: UnitTable,
    supervisor: Supervisor,
}

impl ConfigProvider {
    pub fn new(units: UnitTable, supervisor: Supervisor) -> Self {
        Self { units, supervisor }
    }

    /// What the document configures each *built* unit with.
    ///
    /// Every built unit is named, not only the configured ones: a unit whose
    /// settings were taken out of the document has changed just as much as
    /// one whose settings were edited, and must go back to its defaults.
    fn desired(&self, document: &StateDocument) -> Vec<(UnitName, HashMap<String, Value>)> {
        self.units
            .built()
            .into_iter()
            .map(|name| {
                let config = document
                    .units
                    .iter()
                    .find(|unit| unit.name == name.as_str())
                    .map(|unit| unit.config.clone())
                    .unwrap_or_default();
                (name, config)
            })
            .collect()
    }
}

#[async_trait]
impl Provider for ConfigProvider {
    fn domain(&self) -> &'static str {
        "config"
    }

    fn plan(&self, document: &StateDocument) -> Vec<Change> {
        self.desired(document)
            .into_iter()
            .filter(|(name, config)| &self.units.config(name) != config)
            .map(|(name, _)| {
                Change::update(
                    name.to_string(),
                    "the document configures this unit differently",
                )
            })
            .collect()
    }

    async fn apply(
        &self,
        document: &StateDocument,
        changes: &[Change],
    ) -> Result<(), ProviderError> {
        let desired: HashMap<UnitName, HashMap<String, Value>> =
            self.desired(document).into_iter().collect();

        for change in changes {
            let Ok(name) = UnitName::parse(change.target.clone()) else {
                continue;
            };
            let Some(config) = desired.get(&name) else {
                continue;
            };

            // Recorded first: whatever runs next has to find the settings its
            // handshake will carry already on file, and that is as true of a
            // unit the next provider is about to start as of one being cycled
            // here.
            if !self.units.configure(&name, config.clone()) {
                continue;
            }

            // A unit that is not running needs no cycling — it has not been
            // told anything yet, and will be told this when it starts.
            if self.supervisor.restart(&name) {
                tracing::info!(unit = %name, "reconfigured; running it again");
            }
        }
        Ok(())
    }
}
