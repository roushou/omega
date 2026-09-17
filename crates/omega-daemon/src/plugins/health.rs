//! On-demand health projections contain readiness facts, never private view trees.

use omega_proto::omega::{PluginHealth, PluginReadiness, RenderReadiness, SurfaceHealth};

use super::{PluginRecord, PluginRegistry};
use crate::hub::{Hub, SurfaceRef};

impl PluginRegistry {
    /// Retain validated placement intent even before a plugin can construct it.
    /// Session loss clears instances, but does not erase desired placement intent.
    pub(crate) fn expect_presentations<'a>(
        &self,
        placements: impl Iterator<Item = &'a SurfaceRef>,
    ) {
        let mut records = self.lock();
        for record in records.values_mut() {
            record.placements.clear();
        }

        for placement in placements {
            records
                .entry(placement.plugin.clone())
                .or_insert_with(|| PluginRecord::new(placement.plugin.clone()))
                .placements
                .insert(placement.clone());
        }
    }

    pub(crate) fn health_snapshot(
        &self,
    ) -> (Vec<omega_proto::omega::PluginStatus>, Vec<PluginHealth>) {
        self.lock()
            .values()
            .map(|record| (record.status(), record.health(&self.inner.hub)))
            .unzip()
    }
}

impl PluginRecord {
    fn health(&self, hub: &Hub) -> PluginHealth {
        let mut instances = Vec::new();

        for instance in self.instances.values() {
            let view = hub.view(&instance.key);
            let readiness = view.as_ref().map_or(RenderReadiness::Waiting, |view| {
                view.view.render_readiness()
            });

            instances.push(SurfaceHealth {
                instance: Some(instance.key.wire()),
                surface: instance.surface.to_string(),
                placement: instance
                    .placement
                    .as_ref()
                    .and_then(|address| address.module.as_ref())
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                readiness: readiness as i32,
                render_error: view
                    .as_ref()
                    .map(|view| view.view.render_error.clone())
                    .unwrap_or_default(),
                pending_topics: view
                    .as_ref()
                    .map(|view| view.view.pending_topics.clone())
                    .unwrap_or_default(),
                requested: instance.state.requested().wire(),
                observed: instance.state.observed().wire(),
            });
        }

        for placement in &self.placements {
            if self
                .instances
                .values()
                .any(|instance| instance.placement.as_ref() == Some(placement))
            {
                continue;
            }

            instances.push(SurfaceHealth {
                surface: placement.surface.to_string(),
                placement: placement
                    .module
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                ..Default::default()
            });
        }

        instances.sort_by(|a, b| (&a.surface, &a.placement).cmp(&(&b.surface, &b.placement)));
        let mut surfaces: Vec<_> = self
            .manifest
            .as_ref()
            .map(|manifest| {
                manifest
                    .manifest
                    .surfaces
                    .iter()
                    .map(|surface| surface.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        surfaces.sort();

        let readiness = if self.manifest.is_none() {
            PluginReadiness::Unspecified
        } else if instances
            .iter()
            .any(|instance| instance.readiness == RenderReadiness::Failed as i32)
        {
            PluginReadiness::Failed
        } else if instances.iter().any(|instance| {
            instance.readiness == RenderReadiness::Waiting as i32 || instance.instance.is_none()
        }) {
            PluginReadiness::Waiting
        } else if instances
            .iter()
            .any(|instance| instance.readiness != RenderReadiness::Ready as i32)
        {
            PluginReadiness::Unspecified
        } else if !instances.is_empty() {
            PluginReadiness::Ready
        } else if surfaces.is_empty() {
            PluginReadiness::Background
        } else {
            PluginReadiness::Unplaced
        };

        PluginHealth {
            plugin: self.name.to_string(),
            readiness: readiness as i32,
            surfaces,
            instances,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ManifestStore;
    use crate::plugins::instance::Instance;
    use omega_proto::instance::PresentationSpec;
    use omega_proto::omega::{self, SurfaceKind, ViewTree, presentation};
    use omega_proto::{Manifest, ModuleId, PluginName, Surface, SurfaceId};

    struct Fixture {
        hub: Hub,
        plugins: PluginRegistry,
        name: PluginName,
        placement: SurfaceRef,
    }

    impl Fixture {
        fn new() -> Self {
            let hub = Hub::new();
            let plugins = PluginRegistry::detached(hub.clone());
            let name = "network".parse::<PluginName>().unwrap();
            let surface = "indicator".parse::<SurfaceId>().unwrap();
            let placement = SurfaceRef::module(
                name.clone(),
                surface.clone(),
                "wifi".parse::<ModuleId>().unwrap(),
            );
            plugins
                .adopt(&ManifestStore::from_manifests([Manifest::new(&name, "1")
                    .exposing([Surface::new(&surface, SurfaceKind::Widget)])]));
            Self {
                hub,
                plugins,
                name,
                placement,
            }
        }

        fn health(&self) -> PluginHealth {
            self.plugins.health_snapshot().1.remove(0)
        }

        fn instance(&self) -> Instance {
            let instance = Instance::new(
                self.placement.surface.clone(),
                Default::default(),
                PresentationSpec::try_from(omega::Presentation {
                    kind: Some(presentation::Kind::Embedded(omega::EmbeddedPresentation {
                        placement: "wifi".into(),
                    })),
                })
                .unwrap(),
                None,
                Some(self.placement.clone()),
            )
            .unwrap();
            self.plugins
                .lock()
                .get_mut(&self.name)
                .unwrap()
                .instances
                .insert(instance.key.id.clone(), instance.clone());
            instance
        }
    }

    #[test]
    fn declarations_and_desired_placements_are_distinct_from_live_instances() {
        let f = Fixture::new();
        assert_eq!(f.health().readiness(), PluginReadiness::Unplaced);
        f.plugins
            .expect_presentations(std::iter::once(&f.placement));
        let health = f.health();
        assert_eq!(health.readiness(), PluginReadiness::Waiting);
        assert!(health.instances[0].instance.is_none());
        f.plugins.expect_presentations(std::iter::empty());
        assert_eq!(f.health().readiness(), PluginReadiness::Unplaced);

        f.plugins
            .adopt(&ManifestStore::from_manifests([Manifest::new(
                &f.name, "1",
            )]));
        assert_eq!(f.health().readiness(), PluginReadiness::Background);
    }

    #[tokio::test]
    async fn waiting_empty_ready_and_disconnect_preserve_their_meaning() {
        let f = Fixture::new();
        f.plugins
            .expect_presentations(std::iter::once(&f.placement));
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let guard = f.plugins.connected(&f.name, tx);
        let instance = f.instance();
        assert_eq!(f.health().readiness(), PluginReadiness::Waiting);

        let waiting = ViewTree {
            readiness: RenderReadiness::Waiting as i32,
            pending_topics: vec!["network".into()],
            ..Default::default()
        };
        f.hub
            .publish_view(instance.update(&f.name, waiting))
            .unwrap();
        let before = f.hub.view(&instance.key).unwrap().view.revision;
        assert_eq!(f.health().instances[0].pending_topics, ["network"]);

        f.hub
            .publish_view(instance.update(
                &f.name,
                ViewTree {
                    readiness: RenderReadiness::Ready as i32,
                    ..Default::default()
                },
            ))
            .unwrap();
        assert!(f.hub.view(&instance.key).unwrap().view.revision > before);
        let health = f.health();
        assert_eq!(health.readiness(), PluginReadiness::Ready);
        assert_eq!(
            health.instances[0].observed,
            omega::PresentationState::Hidden as i32
        );
        assert!(health.instances[0].pending_topics.is_empty());

        drop(guard);
        let health = f.health();
        assert_eq!(health.readiness(), PluginReadiness::Waiting);
        assert!(health.instances[0].instance.is_none());
        assert!(f.hub.view(&instance.key).is_none());
    }

    #[test]
    fn legacy_empty_views_and_mixed_instances_do_not_claim_readiness() {
        let f = Fixture::new();
        let first = f.instance();
        f.hub
            .publish_view(first.update(&f.name, ViewTree::default()))
            .unwrap();
        assert_eq!(f.health().readiness(), PluginReadiness::Unspecified);

        f.hub
            .publish_view(first.update(
                &f.name,
                ViewTree {
                    readiness: RenderReadiness::Ready as i32,
                    ..Default::default()
                },
            ))
            .unwrap();
        let second = f.instance();
        f.hub
            .publish_view(second.update(
                &f.name,
                ViewTree {
                    readiness: RenderReadiness::Waiting as i32,
                    pending_topics: vec!["network".into()],
                    ..Default::default()
                },
            ))
            .unwrap();
        assert_eq!(f.health().readiness(), PluginReadiness::Waiting);
        assert_eq!(f.health().instances.len(), 2);
    }
    #[test]
    fn render_failure_is_reported_without_changing_process_lifecycle() {
        let f = Fixture::new();
        let instance = f.instance();
        f.hub
            .publish_view(instance.update(
                &f.name,
                omega::ViewTree {
                    readiness: RenderReadiness::Failed as i32,
                    render_error: "binding capacity exceeded".into(),
                    ..Default::default()
                },
            ))
            .unwrap();
        let health = f.health();
        assert_eq!(health.readiness(), PluginReadiness::Failed);
        assert_eq!(
            health.instances[0].render_error,
            "binding capacity exceeded"
        );
        f.hub
            .publish_view(instance.update(
                &f.name,
                omega::ViewTree {
                    readiness: RenderReadiness::Ready as i32,
                    ..Default::default()
                },
            ))
            .unwrap();
        assert!(f.health().instances[0].render_error.is_empty());
        assert_eq!(f.health().readiness(), PluginReadiness::Ready);
    }
}
