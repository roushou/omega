//! Which units run.
//!
//! Two planes meet here: the workspace decides what is *built*, the document
//! decides what is *enabled*. A built unit the document never mentions runs —
//! putting a crate in the workspace is already a declaration — and naming it
//! with `enabled = false` turns it off without deleting it.

use std::collections::BTreeSet;

use async_trait::async_trait;

use omega_host::Layout;
use omega_proto::UnitName;
use omega_proto::omega::StateDocument;

use crate::reconcile::{Change, Provider, ProviderError};
use crate::supervisor::{Supervisor, UnitLog, UnitSpec};

#[derive(Debug)]
pub struct UnitProvider {
    supervisor: Supervisor,
    layout: Layout,
    /// Every unit the build produced.
    built: BTreeSet<UnitName>,
}

impl UnitProvider {
    pub fn new(
        supervisor: Supervisor,
        layout: &Layout,
        built: impl IntoIterator<Item = UnitName>,
    ) -> Self {
        Self {
            supervisor,
            layout: layout.clone(),
            built: built.into_iter().collect(),
        }
    }

    /// The units the document wants running: everything built, minus what it
    /// disables. A document naming a unit that was never built is a mistake
    /// worth reporting rather than silently ignoring.
    fn desired(&self, document: &StateDocument) -> BTreeSet<UnitName> {
        let disabled: BTreeSet<&str> = document
            .units
            .iter()
            .filter(|unit| !unit.enabled)
            .map(|unit| unit.name.as_str())
            .collect();

        self.built
            .iter()
            .filter(|name| !disabled.contains(name.as_str()))
            .cloned()
            .collect()
    }

    fn unknown(&self, document: &StateDocument) -> Vec<String> {
        document
            .units
            .iter()
            .map(|unit| unit.name.clone())
            .filter(|name| !self.built.iter().any(|built| built.as_str() == name))
            .collect()
    }
}

#[async_trait]
impl Provider for UnitProvider {
    fn domain(&self) -> &'static str {
        "units"
    }

    fn plan(&self, document: &StateDocument) -> Vec<Change> {
        let desired = self.desired(document);
        let running: BTreeSet<UnitName> = self.supervisor.running().into_iter().collect();

        let mut changes: Vec<Change> = desired
            .difference(&running)
            .map(|name| Change::create(name.as_str(), "start the unit"))
            .chain(
                running
                    .difference(&desired)
                    .map(|name| Change::delete(name.as_str(), "the document disables this unit")),
            )
            .collect();

        for name in self.unknown(document) {
            changes.push(Change::update(
                name.clone(),
                format!("the document names {name}, which this build does not contain"),
            ));
        }

        changes.sort_by(|a, b| a.target.cmp(&b.target));
        changes
    }

    async fn apply(
        &self,
        _document: &StateDocument,
        changes: &[Change],
    ) -> Result<(), ProviderError> {
        for change in changes {
            let Ok(name) = UnitName::parse(change.target.clone()) else {
                continue;
            };

            match change.action {
                crate::reconcile::Action::Create => {
                    self.supervisor.spawn(
                        UnitSpec::new(name.clone(), self.layout.state_unit_program(&name))
                            .logged(UnitLog::at(self.layout.unit_log(&name))),
                    );
                }
                crate::reconcile::Action::Delete => self.supervisor.stop(&name).await,
                // A unit the build does not contain: reported in the plan,
                // and nothing this provider can do about it.
                crate::reconcile::Action::Update => {
                    tracing::warn!(unit = %name, "{}", change.summary)
                }
            }
        }
        Ok(())
    }
}
