//! Bar composition.
//!
//! The document declares which widget instances a bar is made of; this asks
//! each unit to render the instances that belong to it. A widget surface can
//! be instantiated more than once — two clocks with different formats — so
//! what is reconciled here is *instances*, keyed by the module id the
//! document gave them.
//!
//! After the first render a unit pushes its own updates for each module, the
//! way it always has: the pull exists to hand a unit its instances and their
//! configuration, not to drive its rendering.

use std::collections::BTreeSet;

use async_trait::async_trait;

use omega_proto::omega::{RenderWidget, StateDocument, SurfaceKind, invoke, module, result};
use omega_proto::{ModuleId, SurfaceId, UnitName};

use crate::hub::{Hub, SurfaceRef, ViewUpdate};
use crate::manifest::ManifestStore;
use crate::reconcile::{Action, Change, Provider, ProviderError};
use crate::units::UnitTable;
use std::sync::Arc;

/// One view instance a bar declares.
///
/// Keyed by surface as well as module: a module can place two — the widget in
/// the slot and the panel that pops out of it — and they are rendered
/// separately because they are separate views of the same unit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Instance {
    unit: UnitName,
    module: ModuleId,
    surface: SurfaceId,
}

#[derive(Debug)]
pub struct BarProvider {
    hub: Hub,
    units: UnitTable,
    manifests: Arc<ManifestStore>,
}

impl BarProvider {
    pub fn new(hub: Hub, units: UnitTable, manifests: Arc<ManifestStore>) -> Self {
        Self {
            hub,
            units,
            manifests,
        }
    }

    /// The widget instances the document declares, ignoring modules the
    /// daemon renders itself (a clock is not a unit).
    fn declared(&self, document: &StateDocument) -> BTreeSet<Instance> {
        document
            .bars
            .iter()
            .flat_map(|bar| bar.modules.iter())
            .filter_map(|module| match module.kind.as_ref()? {
                module::Kind::Widget(widget) => {
                    let unit = UnitName::parse(widget.unit.clone()).ok()?;
                    let module = ModuleId::parse(module.id.clone()).ok()?;
                    Some((unit, module, widget))
                }
                _ => None,
            })
            .flat_map(|(unit, module, widget)| {
                // The slot, and the popout when there is one. A surface the
                // document names but the unit does not declare is caught in
                // `surface_of`, which reports rather than guesses.
                [widget.surface.as_str(), widget.panel.as_str()]
                    .into_iter()
                    .enumerate()
                    .filter(|(index, named)| *index == 0 || !named.is_empty())
                    .filter_map(|(_, named)| {
                        Some(Instance {
                            unit: unit.clone(),
                            module: module.clone(),
                            surface: self.surface_of(&unit, named).ok()?,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The instances that already have a view.
    fn rendered(&self) -> BTreeSet<Instance> {
        self.hub
            .view_snapshot()
            .into_iter()
            .filter_map(|update| {
                Some(Instance {
                    unit: update.surface.unit,
                    module: update.surface.module?,
                    surface: update.surface.surface,
                })
            })
            .collect()
    }

    /// The surface a placement means.
    ///
    /// Named, and the unit has to declare it. Unnamed, and the unit's only
    /// widget surface is what naming the unit meant — a unit with several
    /// cannot be addressed by name alone, and the document has to say which
    /// rather than have the daemon guess.
    fn surface_of(&self, unit: &UnitName, named: &str) -> Result<SurfaceId, String> {
        let Some(entry) = self.manifests.get(unit) else {
            return Err(format!("{unit} is not a unit of this build"));
        };

        let widgets: Vec<SurfaceId> = entry
            .manifest
            .surfaces
            .iter()
            .filter(|surface| matches!(surface.declared(), Ok(SurfaceKind::Widget)))
            .filter_map(|surface| surface.surface_id().ok())
            .collect();

        if !named.is_empty() {
            return widgets
                .into_iter()
                .find(|surface| surface.as_str() == named)
                .ok_or_else(|| format!("{unit} declares no widget surface {named:?}"));
        }

        match widgets.as_slice() {
            [surface] => Ok((*surface).clone()),
            [] => Err(format!("{unit} declares no widget surface")),
            many => Err(format!(
                "{unit} declares {} widget surfaces; a module must name the one it places",
                many.len()
            )),
        }
    }

    /// Ask a unit to render one instance, and publish what it answers.
    async fn render(&self, instance: &Instance, document: &StateDocument) -> Result<(), String> {
        let surface = instance.surface.clone();
        let config = Self::config_of(document, &instance.module);

        let outcome = self
            .units
            .request(
                &instance.unit,
                invoke::Op::RenderWidget(RenderWidget {
                    surface_id: surface.to_string(),
                    module_id: instance.module.to_string(),
                    config,
                }),
            )
            .await
            .map_err(|e| e.to_string())?;

        let result::Outcome::View(view) = outcome else {
            return Err(format!("{} answered with no view", instance.unit));
        };

        self.hub.publish_view(ViewUpdate {
            surface: SurfaceRef::module(
                instance.unit.clone(),
                surface.clone(),
                instance.module.clone(),
            ),
            view,
        });

        // The document has taken this surface over; the unit's own single
        // view was from before it knew it had instances.
        self.hub.drop_anonymous(&instance.unit, &surface);
        Ok(())
    }

    fn config_of(
        document: &StateDocument,
        module_id: &ModuleId,
    ) -> std::collections::HashMap<String, omega_proto::omega::Value> {
        document
            .bars
            .iter()
            .flat_map(|bar| bar.modules.iter())
            .filter(|module| module.id == module_id.as_str())
            .find_map(|module| match module.kind.as_ref()? {
                module::Kind::Widget(widget) => Some(widget.config.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl Provider for BarProvider {
    fn domain(&self) -> &'static str {
        "bars"
    }

    fn plan(&self, document: &StateDocument) -> Vec<Change> {
        let declared = self.declared(document);
        let rendered = self.rendered();

        let mut changes: Vec<Change> = declared
            .difference(&rendered)
            .map(|instance| {
                Change::create(
                    instance.module.as_str(),
                    format!("ask {} to render it", instance.unit),
                )
            })
            .chain(rendered.difference(&declared).map(|instance| {
                Change::delete(instance.module.as_str(), "no bar declares this instance")
            }))
            .collect();

        changes.sort_by(|a, b| a.target.cmp(&b.target));
        changes
    }

    async fn apply(
        &self,
        document: &StateDocument,
        changes: &[Change],
    ) -> Result<(), ProviderError> {
        let declared = self.declared(document);

        for change in changes {
            match change.action {
                Action::Create | Action::Update => {
                    let Some(instance) = declared
                        .iter()
                        .find(|instance| instance.module.as_str() == change.target)
                    else {
                        continue;
                    };
                    // A unit that is starting, wedged, or missing a surface
                    // does not fail the convergence of everything else.
                    if let Err(e) = self.render(instance, document).await {
                        tracing::warn!(module = %change.target, "cannot render: {e}");
                    }
                }
                Action::Delete => {
                    if let Ok(module) = ModuleId::parse(change.target.clone()) {
                        self.hub.drop_view(&module);
                    }
                }
            }
        }
        Ok(())
    }
}
