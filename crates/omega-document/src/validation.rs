//! Validation shared by build publication and daemon adoption.

use omega_proto::omega::{StateDocument, SurfaceKind, module};
use omega_proto::{Manifest, ModuleId, PluginName, SurfaceId};
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
            let name = manifest.plugin().map_err(Self::cause)?;
            manifest.validate(&name).map_err(Self::cause)?;
            if built.insert(name, manifest).is_some() {
                return Err(Self::error("duplicate built plugin"));
            }
        }
        omega_proto::CommandContracts::validate(built.values().copied()).map_err(Self::cause)?;
        let mut command_hosts = BTreeSet::new();
        for config in &document.command_hosts {
            let policy = omega_proto::host::HostPolicy::try_from(config).map_err(Self::cause)?;
            let name = config.id.parse::<PluginName>().map_err(Self::cause)?;
            if !command_hosts.insert(name.clone()) {
                return Err(Self::error("duplicate command host configuration"));
            }
            if !built.get(&name).is_some_and(|manifest| {
                manifest.host_kind == omega_proto::omega::HostKind::Commands as i32
            }) {
                return Err(Self::error(format!(
                    "{} is not a built command host",
                    policy.id
                )));
            }
        }
        for (name, manifest) in &built {
            if manifest.host_kind == omega_proto::omega::HostKind::Commands as i32
                && !command_hosts.contains(name)
            {
                return Err(Self::error(format!(
                    "command host {name} requires an explicit lifetime configuration"
                )));
            }
        }
        let mut plugins = BTreeSet::new();
        for plugin in &document.plugins {
            let name = plugin.name.parse::<PluginName>().map_err(Self::cause)?;
            if !built.get(&name).is_some_and(|manifest| {
                manifest.host_kind == omega_proto::omega::HostKind::Plugin as i32
            }) {
                return Err(Self::error(format!("unknown plugin {name}")));
            }
            if !plugins.insert(name) {
                return Err(Self::error("duplicate configured plugin"));
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
                if let omega_proto::omega::action::Kind::InvokePlugin(call) = kind {
                    let manifest = built
                        .values()
                        .find(|manifest| {
                            (call.plugin.is_empty() || call.plugin == manifest.name)
                                && manifest
                                    .commands
                                    .iter()
                                    .any(|command| command.id == call.command)
                        })
                        .ok_or_else(|| {
                            Self::error(format!(
                                "schedule {:?}: unknown command {}",
                                schedule.id, call.command
                            ))
                        })?;
                    let plugin = &manifest.name;
                    if !manifest
                        .commands
                        .iter()
                        .any(|command| command.id == call.command)
                    {
                        return Err(Self::error(format!(
                            "schedule {:?}: {plugin} declares no command {:?}",
                            schedule.id, call.command
                        )));
                    }
                }
            }
            if schedule.id.is_empty() || !schedules.insert(&schedule.id) {
                return Err(Self::error("empty or duplicate schedule id"));
            }
        }
        if !document.shell_json.is_empty() {
            return Err(Self::error(
                "shell payload requires validation by its integration",
            ));
        }
        let mut bars = BTreeSet::new();
        let mut modules = BTreeSet::new();
        for bar in &document.bars {
            if bar.id.is_empty() || !bars.insert(&bar.id) {
                return Err(Self::error("empty or duplicate bar id"));
            }
            for entry in &bar.modules {
                entry.id.parse::<ModuleId>().map_err(Self::cause)?;
                entry
                    .id
                    .parse::<omega_proto::instance::PlacementId>()
                    .map_err(Self::cause)?;
                if !modules.insert(&entry.id) {
                    return Err(Self::error("module ids must be unique across bars"));
                }
                let Some(module::Kind::Widget(widget)) = &entry.kind else {
                    return Err(Self::error("only widget modules have an instance provider"));
                };
                let name = widget.plugin.parse::<PluginName>().map_err(Self::cause)?;
                let manifest = built
                    .get(&name)
                    .ok_or_else(|| Self::error(format!("unknown widget plugin {name}")))?;
                let surface = Self::surface(manifest, &widget.surface)?;
                if !widget.panel.is_empty() && Self::surface(manifest, &widget.panel)? == surface {
                    return Err(Self::error(
                        "widget and panel must address different surfaces",
                    ));
                }
            }
        }
        for entry in &document.presentations {
            entry
                .id
                .parse::<omega_proto::instance::PlacementId>()
                .map_err(Self::cause)?;
            if !modules.insert(&entry.id) {
                return Err(Self::error(
                    "presentation and bar placement ids must be unique",
                ));
            }
            let plugin = entry.plugin.parse::<PluginName>().map_err(Self::cause)?;
            let manifest = built
                .get(&plugin)
                .ok_or_else(|| Self::error(format!("unknown plugin {plugin}")))?;
            Self::surface(manifest, &entry.surface)?;
            let specification = omega_proto::instance::PresentationSpec::try_from(
                entry
                    .presentation
                    .clone()
                    .ok_or_else(|| Self::error("presentation kind is required"))?,
            )
            .map_err(Self::cause)?;
            match specification.wire().kind.as_ref().unwrap() {
                omega_proto::omega::presentation::Kind::Window(window) => {
                    if window.app_id != format!("org.omega.{plugin}") {
                        return Err(Self::error(
                            "window application identity belongs to its plugin",
                        ));
                    }
                }
                omega_proto::omega::presentation::Kind::Overlay(_) => {}
                _ => {
                    return Err(Self::error(
                        "independent presentations must be windows or overlays",
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
                    plugin: manifest.name.clone(),
                    requested: named.into(),
                    available: widgets,
                });
        }
        match widgets.as_slice() {
            [surface] => Ok(surface.clone()),
            _ => Err(ValidationError::AmbiguousSurface {
                plugin: manifest.name.clone(),
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
    #[error("{plugin} declares no widget {requested:?}")]
    MissingSurface {
        plugin: String,
        requested: String,
        available: Vec<SurfaceId>,
    },
    #[error("{plugin} requires an explicit widget surface")]
    AmbiguousSurface {
        plugin: String,
        available: Vec<SurfaceId>,
    },
}
