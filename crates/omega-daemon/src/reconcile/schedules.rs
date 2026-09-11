//! Timer declarations are planned here; their tasks remain owned by Schedules.

use crate::reconcile::ProviderError;
use crate::schedule::Schedules;
use omega_proto::omega::{Schedule, StateDocument};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct ScheduleProvider {
    schedules: Schedules,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScheduleChange {
    Set(Schedule),
    Remove(String),
}

impl ScheduleChange {
    pub fn id(&self) -> &str {
        match self {
            Self::Set(schedule) => &schedule.id,
            Self::Remove(id) => id,
        }
    }
}

impl ScheduleProvider {
    pub fn new(schedules: Schedules) -> Self {
        Self { schedules }
    }

    pub fn plan(&self, document: &StateDocument) -> Result<Vec<ScheduleChange>, ProviderError> {
        let mut declared = BTreeMap::new();
        for schedule in &document.schedules {
            schedule.parsed().map_err(Self::error)?;
            if let Some(action) = &schedule.action {
                action.validate().map_err(Self::error)?;
            }
            if schedule.id.is_empty() || declared.insert(schedule.id.as_str(), schedule).is_some() {
                return Err(Self::error("empty or duplicate schedule id"));
            }
        }
        let running: BTreeMap<_, _> = self
            .schedules
            .declared()
            .into_iter()
            .map(|schedule| (schedule.id.clone(), schedule))
            .collect();
        let mut changes: Vec<_> = declared
            .iter()
            .filter(|(id, schedule)| running.get(**id) != Some(*schedule))
            .map(|(_, schedule)| ScheduleChange::Set((*schedule).clone()))
            .chain(
                running
                    .keys()
                    .filter(|id| !declared.contains_key(id.as_str()))
                    .cloned()
                    .map(ScheduleChange::Remove),
            )
            .collect();
        changes.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(changes)
    }

    pub async fn apply(&self, changes: &[ScheduleChange]) -> Result<(), ProviderError> {
        for change in changes {
            tracing::info!(schedule = change.id(), "converging schedule");
            match change {
                ScheduleChange::Set(schedule) => {
                    self.schedules.start(schedule).await.map_err(Self::error)?
                }
                ScheduleChange::Remove(id) => self.schedules.stop(id).await,
            }
        }
        Ok(())
    }

    fn error(error: impl std::fmt::Display) -> ProviderError {
        ProviderError::new("schedules", error.to_string())
    }
}
