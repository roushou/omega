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
}

#[derive(Debug)]
pub struct BarProvider {
    units: UnitTable,
    manifests: Arc<ManifestStore>,
}

impl BarProvider {
    pub fn new(units: UnitTable, manifests: Arc<ManifestStore>) -> Self {
        Self { units, manifests }
    }

    fn declared(
        &self,
        document: &StateDocument,
    ) -> Result<BTreeMap<SurfaceRef, HashMap<String, Value>>, ProviderError> {
        let mut instances = BTreeMap::new();
        for bar in &document.bars {
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
        let mut changes = Vec::new();
        for (address, config) in &desired {
            if installed.get(address) != Some(config) {
                changes.push(InstanceChange {
                    action: if installed.contains_key(address) {
                        Action::Update
                    } else {
                        Action::Create
                    },
                    address: address.clone(),
                    config: config.clone(),
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
            });
        }
        Ok(changes)
    }

    pub async fn apply(&self, changes: &[InstanceChange]) -> Result<(), ProviderError> {
        let mut errors = Vec::new();
        for change in changes {
            let outcome = match change.action {
                Action::Create | Action::Update => {
                    self.units
                        .configure_instance(&change.address, change.config.clone())
                        .await
                }
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
        ProviderError::new("bars", error.to_string())
    }
}
