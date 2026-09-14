//! Reconcile the specifications installed in each live unit session.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use omega_proto::omega::{StateDocument, Value, module};
use omega_proto::{ModuleId, SurfaceId, UnitName};

use crate::hub::SurfaceRef;
use crate::manifest::ManifestStore;
use crate::reconcile::{Action, ProviderError};
use crate::units::UnitTable;

#[derive(Debug, Clone)]
pub struct InstanceChange {
    pub action: Action,
    pub address: SurfaceRef,
    pub config: HashMap<String, Value>,
    pub presentation: Option<omega_proto::instance::PresentationSpec>,
    pub anchor: Option<SurfaceRef>,
}

#[derive(Debug)]
pub struct PresentationProvider {
    units: UnitTable,
    manifests: Arc<ManifestStore>,
}

impl PresentationProvider {
    pub fn new(units: UnitTable, manifests: Arc<ManifestStore>) -> Self {
        Self { units, manifests }
    }

    fn declared(
        &self,
        document: &StateDocument,
    ) -> Result<BTreeMap<SurfaceRef, HashMap<String, Value>>, ProviderError> {
        let mut instances = BTreeMap::new();
        let compiled = omega_omarchy::shell::CompiledShell::of(document).map_err(Self::error)?;
        let shell_bars: Vec<_> = compiled
            .as_ref()
            .map(|shell| shell.bar().clone())
            .into_iter()
            .collect();
        for bar in document.bars.iter().chain(&shell_bars) {
            for module in &bar.modules {
                let widget = match &module.kind {
                    Some(module::Kind::Widget(widget)) => widget,
                    Some(
                        module::Kind::Clock(_)
                        | module::Kind::Battery(_)
                        | module::Kind::Workspaces(_)
                        | module::Kind::Tray(_),
                    ) => return Err(Self::error("only widget modules have an instance provider")),
                    None => return Err(Self::error("module has no kind")),
                };
                let unit = UnitName::parse(&widget.unit).map_err(Self::error)?;
                let module = ModuleId::parse(&module.id).map_err(Self::error)?;
                for named in std::iter::once(widget.surface.as_str())
                    .chain((!widget.panel.is_empty()).then_some(widget.panel.as_str()))
                {
                    let surface = self.surface_of(&unit, named)?;
                    let address = SurfaceRef::module(unit.clone(), surface, module.clone());
                    if instances.insert(address, widget.config.clone()).is_some() {
                        return Err(Self::error("duplicate view instance"));
                    }
                }
            }
        }
        for entry in &document.presentations {
            let unit = UnitName::parse(&entry.unit).map_err(Self::error)?;
            let surface = self.surface_of(&unit, &entry.surface)?;
            let address = SurfaceRef::module(
                unit,
                surface,
                ModuleId::parse(&entry.id).map_err(Self::error)?,
            );
            if instances.insert(address, entry.config.clone()).is_some() {
                return Err(Self::error("duplicate presentation instance"));
            }
        }
        Ok(instances)
    }

    fn surface_of(&self, unit: &UnitName, named: &str) -> Result<SurfaceId, ProviderError> {
        let entry = self
            .manifests
            .get(unit)
            .ok_or_else(|| Self::error(format!("unknown unit {unit}")))?;
        omega_document::DocumentValidation::surface(&entry.manifest, named).map_err(Self::error)
    }

    pub fn plan(&self, document: &StateDocument) -> Result<Vec<InstanceChange>, ProviderError> {
        let desired = self.declared(document)?;
        let installed = self.units.instances();
        let compiled = omega_omarchy::shell::CompiledShell::of(document).map_err(Self::error)?;
        let shell_bars = compiled
            .as_ref()
            .map(|shell| shell.bar().clone())
            .into_iter()
            .collect::<Vec<_>>();
        let mut anchors = BTreeMap::new();
        for bar in document.bars.iter().chain(&shell_bars) {
            for entry in &bar.modules {
                if let Some(module::Kind::Widget(widget)) = &entry.kind
                    && !widget.panel.is_empty()
                {
                    let unit = UnitName::parse(&widget.unit).map_err(Self::error)?;
                    let placement = ModuleId::parse(&entry.id).map_err(Self::error)?;
                    anchors.insert(
                        SurfaceRef::module(
                            unit.clone(),
                            self.surface_of(&unit, &widget.panel)?,
                            placement.clone(),
                        ),
                        SurfaceRef::module(
                            unit.clone(),
                            self.surface_of(&unit, &widget.surface)?,
                            placement,
                        ),
                    );
                }
            }
        }
        let mut changes = Vec::new();
        for (address, config) in &desired {
            let anchor = anchors.get(address).cloned();
            let presentation = document
                .presentations
                .iter()
                .find(|entry| {
                    address
                        .module
                        .as_ref()
                        .is_some_and(|module| module.as_str() == entry.id)
                })
                .map(|entry| {
                    omega_proto::instance::PresentationSpec::parse(
                        entry
                            .presentation
                            .clone()
                            .ok_or_else(|| Self::error("presentation is required"))?,
                    )
                    .map_err(Self::error)
                })
                .transpose()?;
            if self.units.anchor_at(address) != anchor
                || installed.get(address) != Some(config)
                || presentation
                    .as_ref()
                    .is_some_and(|spec| self.units.presentation_at(address).as_ref() != Some(spec))
            {
                changes.push(InstanceChange {
                    action: if installed.contains_key(address) {
                        Action::Update
                    } else {
                        Action::Create
                    },
                    address: address.clone(),
                    config: config.clone(),
                    presentation,
                    anchor,
                });
            }
        }
        for address in installed
            .keys()
            .filter(|address| !desired.contains_key(*address))
        {
            changes.push(InstanceChange {
                action: Action::Delete,
                address: address.clone(),
                config: HashMap::new(),
                presentation: None,
                anchor: None,
            });
        }
        changes.sort_by_key(|change| change.anchor.is_some());
        Ok(changes)
    }

    pub async fn apply(&self, changes: &[InstanceChange]) -> Result<(), ProviderError> {
        let mut errors = Vec::new();
        for change in changes {
            let outcome = match change.action {
                Action::Create | Action::Update if change.anchor.is_some() => {
                    self.units
                        .configure_popup(
                            &change.address,
                            change.config.clone(),
                            change.anchor.as_ref().expect("popup anchor"),
                        )
                        .await
                }
                Action::Create | Action::Update => match &change.presentation {
                    Some(presentation) => {
                        self.units
                            .configure_presentation(
                                &change.address,
                                change.config.clone(),
                                presentation.clone(),
                            )
                            .await
                    }
                    None => {
                        self.units
                            .configure_instance(&change.address, change.config.clone())
                            .await
                    }
                },
                Action::Delete => self.units.remove_instance(&change.address).await,
            };
            if let Err(error) = outcome {
                errors.push(format!("{}: {error}", change.address));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(Self::error(errors.join("; ")))
        }
    }

    fn error(error: impl std::fmt::Display) -> ProviderError {
        ProviderError::new("presentations", error.to_string())
    }
}
