//! Validation shared by build publication and daemon adoption.

use omega_proto::omega::{StateDocument, SurfaceKind, module};
use omega_proto::{Manifest, ModuleId, SurfaceId, UnitName};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct DocumentValidation;

impl DocumentValidation {
    pub fn validate<'a>(
        document: &StateDocument,
        manifests: impl IntoIterator<Item = &'a Manifest>,
    ) -> Result<(), ValidationError> {
        let mut built = BTreeMap::new();
        for manifest in manifests {
            let name = manifest.unit().map_err(Self::cause)?;
            manifest.validate(&name).map_err(Self::cause)?;
            if built.insert(name, manifest).is_some() {
                return Err(Self::error("duplicate built unit"));
            }
        }
        let mut units = BTreeSet::new();
        for unit in &document.units {
            let name = UnitName::parse(&unit.name).map_err(Self::cause)?;
            if !built.contains_key(&name) {
                return Err(Self::error(format!("unknown unit {name}")));
            }
            if !units.insert(name) {
                return Err(Self::error("duplicate configured unit"));
            }
        }
        if !document.monitors.is_empty()
            || !document.workspaces.is_empty()
            || !document.settings.is_empty()
            || !document.policies.is_empty()
            || !document.keybinds.is_empty()
        {
            return Err(Self::error(
                "monitors, workspaces, settings, policies and keybinds have no desired-state provider",
            ));
        }
        Self::environment(document)?;
        let mut schedules = BTreeSet::new();
        for schedule in &document.schedules {
            schedule.parsed().map_err(Self::cause)?;
            if let Some(action) = &schedule.action {
                let kind = action
                    .validate()
                    .map_err(|error| Self::error(format!("schedule {:?}: {error}", schedule.id)))?;
                if let omega_proto::omega::action::Kind::InvokeUnit(call) = kind {
                    let unit = UnitName::parse(&call.unit).map_err(Self::cause)?;
                    let manifest = built.get(&unit).ok_or_else(|| {
                        Self::error(format!("schedule {:?}: unknown unit {unit}", schedule.id))
                    })?;
                    if !manifest.surfaces.iter().any(|surface| {
                        surface.id == call.command && surface.kind == SurfaceKind::Command as i32
                    }) {
                        return Err(Self::error(format!(
                            "schedule {:?}: {unit} declares no command {:?}",
                            schedule.id, call.command
                        )));
                    }
                }
            }
            if schedule.id.is_empty() || !schedules.insert(&schedule.id) {
                return Err(Self::error("empty or duplicate schedule id"));
            }
        }
        let compiled = crate::shell::CompiledShell::of(document)?;
        let shell_bars: Vec<_> = compiled
            .as_ref()
            .map(|shell| shell.bar().clone())
            .into_iter()
            .collect();
        let mut bars = BTreeSet::new();
        let mut modules = BTreeSet::new();
        for bar in document.bars.iter().chain(&shell_bars) {
            if bar.id.is_empty() || !bars.insert(&bar.id) {
                return Err(Self::error("empty or duplicate bar id"));
            }
            for entry in &bar.modules {
                ModuleId::parse(&entry.id).map_err(Self::cause)?;
                if !modules.insert(&entry.id) {
                    return Err(Self::error("module ids must be unique across bars"));
                }
                let Some(module::Kind::Widget(widget)) = &entry.kind else {
                    return Err(Self::error("only widget modules have an instance provider"));
                };
                let name = UnitName::parse(&widget.unit).map_err(Self::cause)?;
                let manifest = built
                    .get(&name)
                    .ok_or_else(|| Self::error(format!("unknown widget unit {name}")))?;
                let surface = Self::surface(manifest, &widget.surface)?;
                if !widget.panel.is_empty() && Self::surface(manifest, &widget.panel)? == surface {
                    return Err(Self::error(
                        "widget and panel must address different surfaces",
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn environment(document: &StateDocument) -> Result<(), ValidationError> {
        let mut keys = BTreeSet::new();
        for variable in &document.environment {
            let mut bytes = variable.key.bytes();
            if !bytes
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                || variable.value.contains('\0')
                || !keys.insert(&variable.key)
            {
                return Err(Self::error(format!(
                    "invalid or duplicate environment variable {:?}",
                    variable.key
                )));
            }
        }
        Ok(())
    }

    pub fn surface(manifest: &Manifest, named: &str) -> Result<SurfaceId, ValidationError> {
        let mut widgets = Vec::new();
        for surface in &manifest.surfaces {
            if surface.declared().map_err(Self::cause)? == SurfaceKind::Widget {
                widgets.push(surface.surface_id().map_err(Self::cause)?);
            }
        }
        if !named.is_empty() {
            return widgets
                .iter()
                .find(|surface| surface.as_str() == named)
                .cloned()
                .ok_or_else(|| ValidationError::MissingSurface {
                    unit: manifest.name.clone(),
                    requested: named.into(),
                    available: widgets,
                });
        }
        match widgets.as_slice() {
            [surface] => Ok(surface.clone()),
            _ => Err(ValidationError::AmbiguousSurface {
                unit: manifest.name.clone(),
                available: widgets,
            }),
        }
    }

    fn cause(error: impl std::error::Error + Send + Sync + 'static) -> ValidationError {
        ValidationError::Cause(Box::new(error))
    }

    fn error(error: impl std::fmt::Display) -> ValidationError {
        ValidationError::Invalid(error.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("invalid desired state: {0}")]
    Invalid(String),
    #[error("invalid desired state: {0}")]
    Cause(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error(transparent)]
    Shell(#[from] crate::shell::ShellError),
    #[error("{unit} declares no widget {requested:?}")]
    MissingSurface {
        unit: String,
        requested: String,
        available: Vec<SurfaceId>,
    },
    #[error("{unit} requires an explicit widget surface")]
    AmbiguousSurface {
        unit: String,
        available: Vec<SurfaceId>,
    },
}
