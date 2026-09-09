//! What fires on its own.
//!
//! The document declares schedules; this starts, restarts and stops the
//! timers behind them. The timers themselves live in [`Schedules`], which
//! outlives a pass — a provider is rebuilt every time the daemon converges,
//! and anything it owned would be rebuilt with it.
//!
//! A schedule is *updated* by being replaced: there is no way to change a
//! running timer's period, and no reason to want one. What matters is that
//! the schedules a pass leaves alone keep ticking, so an unrelated
//! convergence — a unit connecting, a bar re-rendering — does not reset every
//! clock on the machine.

use std::collections::BTreeMap;

use async_trait::async_trait;

use omega_proto::omega::{Schedule, StateDocument};

use crate::reconcile::{Action, Change, Provider, ProviderError};
use crate::schedule::Schedules;

#[derive(Debug)]
pub struct ScheduleProvider {
    schedules: Schedules,
}

impl ScheduleProvider {
    pub fn new(schedules: Schedules) -> Self {
        Self { schedules }
    }

    /// The schedules the document declares, by id.
    ///
    /// Last one wins where a document declares an id twice, and the plan says
    /// so — a duplicated id is a config mistake that would otherwise show up
    /// as a schedule firing at a cadence nobody wrote.
    fn declared(document: &StateDocument) -> BTreeMap<String, Schedule> {
        document
            .schedules
            .iter()
            .map(|schedule| (schedule.id.clone(), schedule.clone()))
            .collect()
    }

    fn running(&self) -> BTreeMap<String, Schedule> {
        self.schedules
            .declared()
            .into_iter()
            .map(|schedule| (schedule.id.clone(), schedule))
            .collect()
    }
}

#[async_trait]
impl Provider for ScheduleProvider {
    fn domain(&self) -> &'static str {
        "schedules"
    }

    fn plan(&self, document: &StateDocument) -> Vec<Change> {
        let declared = Self::declared(document);
        let running = self.running();

        let mut changes = Vec::new();

        for (id, schedule) in &declared {
            match running.get(id) {
                None => changes.push(Change::create(
                    id.clone(),
                    format!("fire {}", schedule.cadence),
                )),
                // Compared as a whole declaration rather than by cadence: an
                // action that changed is as much a different schedule as a
                // period that did.
                Some(current) if current != schedule => changes.push(Change::update(
                    id.clone(),
                    format!("fire {} instead", schedule.cadence),
                )),
                Some(_) => {}
            }
        }

        changes.extend(
            running
                .keys()
                .filter(|id| !declared.contains_key(*id))
                .map(|id| Change::delete(id.clone(), "the document no longer declares it")),
        );

        changes.sort_by(|a, b| a.target.cmp(&b.target));
        changes
    }

    async fn apply(
        &self,
        document: &StateDocument,
        changes: &[Change],
    ) -> Result<(), ProviderError> {
        let declared = Self::declared(document);

        for change in changes {
            match change.action {
                Action::Create | Action::Update => {
                    let Some(schedule) = declared.get(&change.target) else {
                        continue;
                    };
                    // A cadence the grammar cannot read is reported and
                    // skipped. The rest of the document still converges: one
                    // mistyped period should not cost somebody every other
                    // schedule on the machine.
                    if let Err(e) = self.schedules.start(schedule) {
                        tracing::error!(
                            schedule = %schedule.id,
                            error = %e,
                            "the schedule will not fire",
                        );
                    }
                }
                Action::Delete => self.schedules.stop(&change.target),
            }
        }

        Ok(())
    }
}
