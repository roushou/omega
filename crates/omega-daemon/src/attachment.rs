//! Renderer scopes, negotiated features, and revocable instance permits.

use crate::hub::ViewUpdate;
use crate::refusal::RefusableResult;
use omega_proto::instance::{InstanceKey, PlacementId, PresentationSpec};
use omega_proto::omega::{AttachRenderer, RendererFeature, attach_renderer};
use omega_proto::{PluginName, Refusal, SurfaceId};

#[derive(Debug, Clone)]
pub(crate) struct Attachment {
    pub(crate) scope: Scope,
    pub(crate) active: std::sync::Arc<std::sync::atomic::AtomicBool>,
    features: Vec<RendererFeature>,
    fingerprint: Option<omega_proto::instance::RendererFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Scope {
    Plugin(PluginName),
    Placement {
        plugin: PluginName,
        surface: SurfaceId,
        placement: PlacementId,
    },
}

impl Attachment {
    pub(crate) fn from_request(request: &AttachRenderer) -> Result<Self, Refusal> {
        if request.features.len() > 16 {
            return Err(Refusal::invalid("too many renderer features"));
        }
        let features: Vec<_> = request
            .features
            .iter()
            .map(|feature| {
                RendererFeature::try_from(*feature)
                    .map_err(|_| Refusal::unimplemented("unknown renderer feature"))
            })
            .collect::<Result<_, _>>()?;
        for required in [
            RendererFeature::Instances,
            RendererFeature::ScopedInteractions,
            RendererFeature::LocalMessages,
            RendererFeature::ControlledInputs,
        ] {
            if !features.contains(&required) {
                return Err(Refusal::precondition(format!(
                    "renderer requires {}",
                    required.as_str_name()
                )));
            }
        }
        let scope = match request
            .scope
            .as_ref()
            .ok_or_else(|| Refusal::invalid("renderer scope is required"))?
        {
            attach_renderer::Scope::Plugin(plugin) => {
                Scope::Plugin(plugin.parse::<PluginName>().or_refuse()?)
            }
            attach_renderer::Scope::Placement(placement) => Scope::Placement {
                plugin: placement.plugin.parse::<PluginName>().or_refuse()?,
                surface: placement.surface.parse::<SurfaceId>().or_refuse()?,
                placement: placement.placement.parse::<PlacementId>().or_refuse()?,
            },
        };
        let fingerprint = if request.build_fingerprint.is_empty() {
            None
        } else {
            Some(
                request
                    .build_fingerprint
                    .parse::<omega_proto::instance::RendererFingerprint>()
                    .or_refuse()?,
            )
        };
        Ok(Self {
            fingerprint,
            scope,
            features,
            active: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        })
    }
    pub(crate) fn description(&self) -> AttachRenderer {
        use omega_proto::omega::PlacementAttachment;
        AttachRenderer {
            scope: Some(match &self.scope {
                Scope::Plugin(plugin) => attach_renderer::Scope::Plugin(plugin.to_string()),
                Scope::Placement {
                    plugin,
                    surface,
                    placement,
                } => attach_renderer::Scope::Placement(PlacementAttachment {
                    plugin: plugin.to_string(),
                    surface: surface.to_string(),
                    placement: placement.to_string(),
                }),
            }),
            features: self.features.iter().map(|f| *f as i32).collect(),
            build_fingerprint: self
                .fingerprint
                .as_ref()
                .map(|f| f.as_str().to_owned())
                .unwrap_or_default(),
        }
    }
    pub(crate) fn plugin(&self) -> &PluginName {
        match &self.scope {
            Scope::Plugin(plugin) | Scope::Placement { plugin, .. } => plugin,
        }
    }
    pub(crate) fn accepts(&self, view: &ViewUpdate) -> bool {
        if !self.active.load(std::sync::atomic::Ordering::Acquire) {
            return false;
        }
        match &self.scope {
            Scope::Plugin(plugin) => {
                &view.surface.plugin == plugin
                    && matches!(
                        view.presentation.kind,
                        Some(
                            omega_proto::omega::presentation::Kind::Window(_)
                                | omega_proto::omega::presentation::Kind::Overlay(_)
                        )
                    )
            }
            Scope::Placement {
                plugin,
                surface,
                placement,
            } => {
                &view.surface.plugin == plugin
                    && matches!(
                        view.presentation.kind,
                        Some(
                            omega_proto::omega::presentation::Kind::Embedded(_)
                                | omega_proto::omega::presentation::Kind::Popup(_)
                        )
                    )
                    && &view.surface.surface == surface
                    && view
                        .surface
                        .module
                        .as_ref()
                        .is_some_and(|module| module.as_str() == placement.as_str())
            }
        }
    }
    pub(crate) fn validate(&self, view: &ViewUpdate) -> Result<(), Refusal> {
        let presentation = PresentationSpec::try_from(view.presentation.clone()).or_refuse()?;
        if !self.features.contains(&presentation.feature()) {
            return Err(Refusal::unimplemented(
                "renderer does not support this presentation",
            ));
        }
        if !self.features.contains(&RendererFeature::ResolvedNavigation) {
            let mut nodes: Vec<_> = view.view.root.iter().collect();
            while let Some(node) = nodes.pop() {
                if !node.navigation_target.is_empty() {
                    return Err(Refusal::unimplemented(
                        "renderer does not support resolved navigation",
                    ));
                }
                nodes.extend(&node.children);
            }
        }
        if !self.features.contains(&RendererFeature::KeyboardShortcuts) {
            let mut nodes: Vec<_> = view.view.root.iter().collect();
            while let Some(node) = nodes.pop() {
                if !node.shortcuts.is_empty() {
                    return Err(Refusal::unimplemented(
                        "renderer does not support keyboard shortcuts",
                    ));
                }
                nodes.extend(&node.children);
            }
        }
        Ok(())
    }
    pub(crate) fn metadata(
        &self,
        snapshot: &omega_proto::omega::InstanceSnapshot,
    ) -> Result<ViewUpdate, Refusal> {
        let module = match &self.scope {
            Scope::Plugin(_) => None,
            Scope::Placement { placement, .. } => {
                Some(omega_proto::ModuleId::try_from(placement.as_str()).or_refuse()?)
            }
        };
        Ok(ViewUpdate {
            instance: InstanceKey::try_from(
                snapshot
                    .instance
                    .as_ref()
                    .ok_or_else(|| Refusal::invalid("instance identity is required"))?,
            )
            .or_refuse()?,
            surface: crate::hub::SurfaceRef {
                plugin: snapshot.plugin.parse::<PluginName>().or_refuse()?,
                surface: snapshot.surface.parse::<SurfaceId>().or_refuse()?,
                module,
            },
            presentation: snapshot
                .presentation
                .clone()
                .ok_or_else(|| Refusal::invalid("snapshot requires presentation"))?,
            requested: snapshot.requested,
            observed: snapshot.observed,
            destroyed: snapshot.destroyed,
            view: omega_proto::omega::ViewTree {
                root: None,
                revision: snapshot.view.as_ref().map_or(0, |view| view.revision),
                ..Default::default()
            },
        })
    }
    pub(crate) fn authorize(
        &self,
        hub: &crate::hub::Hub,
        key: &InstanceKey,
    ) -> Result<(), Refusal> {
        let view = hub
            .view(key)
            .ok_or_else(|| Refusal::precondition("unknown or expired instance"))?;
        if !self.accepts(&view) {
            return Err(Refusal::denied(
                "instance is outside this renderer attachment",
            ));
        }
        self.validate(&view)
    }
}

pub(crate) type RendererAttachment = std::sync::Arc<std::sync::Mutex<Option<Attachment>>>;

pub(crate) struct InstancePermit {
    pub(crate) key: InstanceKey,
    active: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl InstancePermit {
    pub(crate) fn new(attachment: &Attachment, key: InstanceKey) -> Self {
        Self {
            key,
            active: attachment.active.clone(),
        }
    }
    pub(crate) fn validate(&self) -> Result<(), Refusal> {
        if self.active.load(std::sync::atomic::Ordering::Acquire) {
            Ok(())
        } else {
            Err(Refusal::precondition(
                "renderer attachment has been replaced",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_proto::omega::{
        InstanceRef, InstanceSnapshot, Presentation, Shortcut, ViewNode, WindowPresentation,
        presentation,
    };

    #[test]
    fn keyboard_features_are_required_only_for_views_using_them() {
        let mut attachment = Attachment::from_request(&AttachRenderer {
            scope: Some(attach_renderer::Scope::Plugin("example".into())),
            features: [
                RendererFeature::Instances,
                RendererFeature::ScopedInteractions,
                RendererFeature::LocalMessages,
                RendererFeature::ControlledInputs,
                RendererFeature::Windows,
            ]
            .map(|f| f as i32)
            .to_vec(),
            ..Default::default()
        })
        .unwrap();
        let snapshot = InstanceSnapshot {
            instance: Some(InstanceRef {
                id: "window".into(),
                incarnation: "one".into(),
            }),
            plugin: "example".into(),
            surface: "main".into(),
            presentation: Some(Presentation {
                kind: Some(presentation::Kind::Window(WindowPresentation {
                    width: 400,
                    height: 300,
                    min_width: 1,
                    min_height: 1,
                    app_id: "org.omega.example".into(),
                    ..Default::default()
                })),
            }),
            ..Default::default()
        };
        let mut view = attachment.metadata(&snapshot).unwrap();
        assert!(attachment.validate(&view).is_ok());
        view.view.root = Some(ViewNode {
            children: vec![ViewNode {
                shortcuts: vec![Shortcut {
                    key: "key:Escape".into(),
                    event: "shortcut:0".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        });
        assert!(attachment.validate(&view).is_err());
        attachment.features.push(RendererFeature::KeyboardShortcuts);
        assert!(attachment.validate(&view).is_ok());
        view.view.root.as_mut().unwrap().children[0].navigation_target = "results".into();
        assert!(attachment.validate(&view).is_err());
        attachment
            .features
            .push(RendererFeature::ResolvedNavigation);
        assert!(attachment.validate(&view).is_ok());
    }
}
