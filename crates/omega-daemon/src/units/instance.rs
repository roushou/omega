//! Instance ownership and presentation state live under the unit table's lock.

use super::UnitToken;
use super::presentation_state::{Observation, PresentationState, Visibility};
use crate::hub::{SurfaceRef, ViewUpdate};
use omega_proto::instance::{
    IncarnationId, InstanceId, InstanceKey, PresentationSpec, SingletonId,
};
use omega_proto::omega::{self, Value, ViewTree};
use omega_proto::{SurfaceId, UnitName};
use prost::Message;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(crate) struct Instance {
    pub key: InstanceKey,
    pub lifecycle: std::sync::Arc<tokio::sync::Mutex<()>>,
    pub surface: SurfaceId,
    pub placement: Option<SurfaceRef>,
    pub singleton: Option<SingletonId>,
    pub config: HashMap<String, Value>,
    pub presentation: PresentationSpec,
    pub(super) state: PresentationState,
    pub ready: bool,
}

impl Instance {
    pub(crate) fn new(
        surface: SurfaceId,
        config: HashMap<String, Value>,
        presentation: PresentationSpec,
        singleton: Option<SingletonId>,
        placement: Option<SurfaceRef>,
    ) -> Self {
        let requested = if matches!(
            presentation.wire().kind,
            Some(omega::presentation::Kind::Popup(_))
        ) {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        Self {
            lifecycle: Default::default(),
            key: InstanceKey {
                id: InstanceId::try_from(format!("instance-{}", UnitToken::mint()))
                    .expect("generated ID"),
                incarnation: IncarnationId::try_from(format!("incarnation-{}", UnitToken::mint()))
                    .expect("generated ID"),
            },
            surface,
            config,
            presentation,
            singleton,
            placement,
            state: PresentationState::new(requested, Observation::Known(Visibility::Hidden)),
            ready: false,
        }
    }

    pub(crate) fn config_bytes(&self) -> usize {
        self.config
            .iter()
            .map(|(key, value)| key.len() + value.encoded_len() + 32)
            .sum()
    }

    pub(crate) fn update(&self, unit: &UnitName, view: ViewTree) -> ViewUpdate {
        ViewUpdate {
            destroyed: false,
            instance: self.key.clone(),
            surface: self
                .placement
                .clone()
                .unwrap_or_else(|| SurfaceRef::new(unit.clone(), self.surface.clone())),
            presentation: self.presentation.wire().clone(),
            requested: self.state.requested().wire(),
            observed: self.state.observed().wire(),
            view,
        }
    }
}

impl ViewUpdate {
    pub fn snapshot(&self) -> omega::InstanceSnapshot {
        omega::InstanceSnapshot {
            destroyed: self.destroyed,
            instance: Some(self.instance.wire()),
            unit: self.surface.unit.to_string(),
            surface: self.surface.surface.to_string(),
            presentation: Some(self.presentation.clone()),
            requested: self.requested,
            observed: self.observed,
            view: Some(self.view.clone()),
        }
    }
}
