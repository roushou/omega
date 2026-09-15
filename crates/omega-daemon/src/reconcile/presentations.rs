//! Reconcile the specifications installed in each live unit session.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use omega_proto::omega::{StateDocument, Value, module};
use omega_proto::{ModuleId, SurfaceId, UnitName};

use crate::hub::SurfaceRef;
use crate::manifest::ManifestStore;
use crate::reconcile::{Action, ProviderError};
use crate::units::{InstalledInstance, UnitTable};

#[derive(Debug, Clone)]
pub struct InstanceChange {
    pub action: Action,
    pub address: SurfaceRef,
    pub config: HashMap<String, Value>,
    pub presentation: Option<omega_proto::instance::PresentationSpec>,
    pub anchor: Option<SurfaceRef>,
}

/// Desired construction facts. An absent presentation leaves bar/popup policy to apply.
#[derive(Debug, Clone, PartialEq)]
pub struct DesiredInstance {
    config: HashMap<String, Value>,
    presentation: Option<omega_proto::instance::PresentationSpec>,
    anchor: Option<SurfaceRef>,
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

    /// Compile the host projection once and resolve declared surfaces against stored manifests.
    pub fn prepare(
        &self,
        document: &StateDocument,
    ) -> Result<BTreeMap<SurfaceRef, DesiredInstance>, ProviderError> {
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
                let primary = self.surface_of(&unit, &widget.surface)?;
                for named in std::iter::once(widget.surface.as_str())
                    .chain((!widget.panel.is_empty()).then_some(widget.panel.as_str()))
                {
                    let surface = self.surface_of(&unit, named)?;
                    let address = SurfaceRef::module(unit.clone(), surface.clone(), module.clone());
                    let anchor = (surface != primary)
                        .then(|| SurfaceRef::module(unit.clone(), primary.clone(), module.clone()));
                    if instances
                        .insert(
                            address,
                            DesiredInstance {
                                config: widget.config.clone(),
                                presentation: None,
                                anchor,
                            },
                        )
                        .is_some()
                    {
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
            let presentation = omega_proto::instance::PresentationSpec::parse(
                entry
                    .presentation
                    .clone()
                    .ok_or_else(|| Self::error("presentation is required"))?,
            )
            .map_err(Self::error)?;
            if instances
                .insert(
                    address,
                    DesiredInstance {
                        config: entry.config.clone(),
                        presentation: Some(presentation),
                        anchor: None,
                    },
                )
                .is_some()
            {
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

    /// Compare supplied facts without reading runtime state or interpreting host payloads.
    pub fn plan(
        desired: &BTreeMap<SurfaceRef, DesiredInstance>,
        installed: &BTreeMap<SurfaceRef, InstalledInstance>,
    ) -> Vec<InstanceChange> {
        let mut changes = Vec::new();
        for (address, declaration) in desired {
            let current = installed.get(address);
            if current.is_none_or(|current| {
                current.anchor != declaration.anchor
                    || current.config != declaration.config
                    || declaration
                        .presentation
                        .as_ref()
                        .is_some_and(|spec| &current.presentation != spec)
            }) {
                changes.push(InstanceChange {
                    action: if installed.contains_key(address) {
                        Action::Update
                    } else {
                        Action::Create
                    },
                    address: address.clone(),
                    config: declaration.config.clone(),
                    presentation: declaration.presentation.clone(),
                    anchor: declaration.anchor.clone(),
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
        changes
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

#[cfg(test)]
mod tests {
    use super::*;
    use omega_proto::{IntoValue, instance::PresentationSpec, omega};

    struct Fixture;
    impl Fixture {
        fn address(surface: &str) -> SurfaceRef {
            SurfaceRef::module(
                UnitName::parse("wifi").unwrap(),
                SurfaceId::parse(surface).unwrap(),
                ModuleId::parse("slot").unwrap(),
            )
        }
        fn embedded() -> PresentationSpec {
            PresentationSpec::parse(omega::Presentation {
                kind: Some(omega::presentation::Kind::Embedded(
                    omega::EmbeddedPresentation {
                        placement: "slot".into(),
                    },
                )),
            })
            .unwrap()
        }
        fn window() -> PresentationSpec {
            PresentationSpec::parse(omega::Presentation {
                kind: Some(omega::presentation::Kind::Window(
                    omega::WindowPresentation {
                        app_id: "org.omega.wifi".into(),
                        width: 100,
                        height: 100,
                        min_width: 1,
                        min_height: 1,
                        ..Default::default()
                    },
                )),
            })
            .unwrap()
        }
        fn desired() -> DesiredInstance {
            DesiredInstance {
                config: HashMap::new(),
                presentation: None,
                anchor: None,
            }
        }
        fn installed() -> InstalledInstance {
            InstalledInstance {
                config: HashMap::new(),
                presentation: Self::embedded(),
                anchor: None,
            }
        }
    }

    #[test]
    fn unchanged_facts_produce_no_work_and_each_changed_fact_requires_an_update() {
        let address = Fixture::address("indicator");
        let desired = BTreeMap::from([(address.clone(), Fixture::desired())]);
        let installed = BTreeMap::from([(address.clone(), Fixture::installed())]);
        assert!(PresentationProvider::plan(&desired, &installed).is_empty());
        let mut configured = Fixture::desired();
        configured
            .config
            .insert("label".into(), "changed".into_value());
        let mut anchored = Fixture::desired();
        anchored.anchor = Some(Fixture::address("other"));
        let mut window = Fixture::desired();
        window.presentation = Some(Fixture::window());
        for declaration in [configured, anchored, window] {
            let changes = PresentationProvider::plan(
                &BTreeMap::from([(address.clone(), declaration.clone())]),
                &installed,
            );
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[0].action, Action::Update);
            assert_eq!(changes[0].config, declaration.config);
            assert_eq!(changes[0].anchor, declaration.anchor);
            assert_eq!(changes[0].presentation, declaration.presentation);
        }
        let specified = Fixture::window();
        let desired = BTreeMap::from([(
            address.clone(),
            DesiredInstance {
                presentation: Some(specified.clone()),
                ..Fixture::desired()
            },
        )]);
        let installed = BTreeMap::from([(
            address,
            InstalledInstance {
                presentation: specified,
                ..Fixture::installed()
            },
        )]);
        assert!(PresentationProvider::plan(&desired, &installed).is_empty());
    }

    #[test]
    fn anchors_precede_popups_and_removed_addresses_are_deleted() {
        let anchor = Fixture::address("z-indicator");
        let popup = Fixture::address("a-panel");
        let removed = Fixture::address("removed");
        let desired = BTreeMap::from([
            (
                popup.clone(),
                DesiredInstance {
                    anchor: Some(anchor.clone()),
                    ..Fixture::desired()
                },
            ),
            (anchor.clone(), Fixture::desired()),
        ]);
        let installed = BTreeMap::from([(removed.clone(), Fixture::installed())]);
        let changes = PresentationProvider::plan(&desired, &installed);
        assert_eq!(changes.len(), 3);
        assert_eq!(changes.last().unwrap().address, popup);
        assert!(
            changes
                .iter()
                .any(|change| change.address == anchor && change.action == Action::Create)
        );
        assert!(
            changes
                .iter()
                .any(|change| change.address == removed && change.action == Action::Delete)
        );
        // Anchor readiness is checked by apply, not assumed by pure planning.
        let desired = BTreeMap::from([(
            popup,
            DesiredInstance {
                anchor: Some(anchor),
                ..Fixture::desired()
            },
        )]);
        assert_eq!(
            PresentationProvider::plan(&desired, &BTreeMap::new()).len(),
            1
        );
    }
}
