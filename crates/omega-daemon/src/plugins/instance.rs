//! Instance ownership and presentation state live under the plugin table's lock.

use super::SpawnToken;
use super::presentation_state::{Observation, PresentationState, Visibility};
use crate::hub::{SurfaceRef, ViewUpdate};
use omega_proto::instance::{
    IncarnationId, InstanceId, InstanceKey, PresentationSpec, SingletonId,
};
use omega_proto::omega::{self, Value, ViewTree};
use omega_proto::{PluginName, SurfaceId};
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
    /// Incremented on every Present request, so a stale auto-hide timer cannot
    /// dismiss a newly re-presented instance.
    pub(super) epoch: u64,
    pub ready: bool,
}

impl Instance {
    pub(crate) fn new(
        surface: SurfaceId,
        config: HashMap<String, Value>,
        presentation: PresentationSpec,
        singleton: Option<SingletonId>,
        placement: Option<SurfaceRef>,
    ) -> Result<Self, super::TokenError> {
        let requested = if matches!(
            presentation.wire().kind,
            Some(omega::presentation::Kind::Popup(_))
        ) {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        let id = SpawnToken::mint()?;
        let incarnation = SpawnToken::mint()?;
        Ok(Self {
            lifecycle: Default::default(),
            key: InstanceKey {
                id: InstanceId::try_from(format!("instance-{id}")).expect("generated ID"),
                incarnation: IncarnationId::try_from(format!("incarnation-{incarnation}"))
                    .expect("generated ID"),
            },
            surface,
            config,
            presentation,
            singleton,
            placement,
            state: PresentationState::new(requested, Observation::Known(Visibility::Hidden)),
            epoch: 0,
            ready: false,
        })
    }

    /// The auto-hide timeout, when this is a timed overlay.
    pub(crate) fn timeout_ms(&self) -> Option<u64> {
        match self.presentation.wire().kind.as_ref() {
            Some(omega::presentation::Kind::Overlay(overlay)) if overlay.timeout_ms > 0 => {
                Some(u64::from(overlay.timeout_ms))
            }
            _ => None,
        }
    }

    pub(crate) fn config_bytes(&self) -> usize {
        self.config
            .iter()
            .map(|(key, value)| key.len() + value.encoded_len() + 32)
            .sum()
    }

    pub(crate) fn update(&self, plugin: &PluginName, view: ViewTree) -> ViewUpdate {
        ViewUpdate {
            destroyed: false,
            instance: self.key.clone(),
            surface: self
                .placement
                .clone()
                .unwrap_or_else(|| SurfaceRef::new(plugin.clone(), self.surface.clone())),
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
            plugin: self.surface.plugin.to_string(),
            surface: self.surface.surface.to_string(),
            presentation: Some(self.presentation.clone()),
            requested: self.requested,
            observed: self.observed,
            view: Some(self.view.clone()),
        }
    }
}
