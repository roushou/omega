use super::*;
use omega_proto::omega::{Bar as WireBar, Edge, Module, StateDocument, WidgetModule, module};

/// Deterministic output from one shell declaration.
#[derive(Debug)]
pub struct CompiledShell {
    config: Value,
    bar: WireBar,
}
impl CompiledShell {
    pub fn config(&self) -> &Value {
        &self.config
    }
    pub fn encode(&self) -> Result<String, ShellError> {
        Ok(format!("{}\n", serde_json::to_string_pretty(&self.config)?))
    }
    pub fn bar(&self) -> &WireBar {
        &self.bar
    }
    pub fn of(document: &StateDocument) -> Result<Option<Self>, ShellError> {
        if document.shell_json.is_empty() {
            return Ok(None);
        }
        if !document.bars.is_empty() {
            return Err(ShellError::Invalid(
                "declare shell placements or legacy bars, not both".into(),
            ));
        }
        Shell::decode(&document.shell_json)?.compile().map(Some)
    }
}
impl Shell {
    pub fn compile(&self) -> Result<CompiledShell, ShellError> {
        Self::reserved(&self.extensions, &["version", "bar", "idle", "plugins"])?;
        Self::reserved(
            &self.bar.extensions,
            &["position", "transparent", "centerAnchor", "layout"],
        )?;
        Self::reserved(&self.idle.extensions, &["screensaver", "lock"])?;
        for duration in [self.idle.screensaver, self.idle.lock] {
            if duration.subsec_nanos() != 0 || duration.as_secs() > u32::MAX as u64 {
                return Err(ShellError::Invalid(
                    "idle deadlines must be whole seconds within u32 range".into(),
                ));
            }
        }
        let mut modules = Vec::new();
        let mut seen = std::collections::BTreeMap::new();
        let mut layout = serde_json::Map::new();
        for (section, entries) in [
            ("left", &self.bar.left),
            ("center", &self.bar.center),
            ("right", &self.bar.right),
        ] {
            let mut output = Vec::new();
            for (index, entry) in entries.iter().enumerate() {
                output.push(match entry {
                    BarItem::Native(native) => native.compile()?,
                    BarItem::Plugin(plugin) => {
                        let path = format!("bar.layout.{section}[{index}]");
                        if let Some(first) = seen.insert(&plugin.id, path.clone()) {
                            return Err(ShellError::DuplicatePlacement { id: plugin.id.clone(), first, second: path });
                        }
                        if !plugin.surface.is_empty() {
                            omega_proto::SurfaceId::parse(&plugin.surface).map_err(|e| ShellError::Invalid(e.to_string()))?;
                        }
                        if let Some(panel) = &plugin.panel {
                            omega_proto::SurfaceId::parse(panel).map_err(|e| ShellError::Invalid(e.to_string()))?;
                        }
                        modules.push(Module { id: plugin.id.to_string(), kind: Some(module::Kind::Widget(WidgetModule {
                            unit: plugin.unit.to_string(), surface: plugin.surface.clone(),
                            panel: plugin.panel.clone().unwrap_or_default(),
                            config: plugin.settings.clone().into_iter().collect(),
                        })) });
                        let mut value = serde_json::json!({"id":"omega.view", "unit":plugin.unit.as_str(),
                            "surface":plugin.surface, "module":plugin.id.as_str()});
                        if let Some(panel) = &plugin.panel { value["panel"] = panel.clone().into(); }
                        value
                    }
                });
            }
            layout.insert(section.into(), output.into());
        }
        let mut bar = serde_json::to_value(&self.bar.extensions)?;
        bar["position"] = serde_json::to_value(self.bar.position)?;
        bar["transparent"] = self.bar.transparent.into();
        bar["layout"] = layout.into();
        if let Some(anchor) = &self.bar.center_anchor {
            bar["centerAnchor"] = anchor.clone().into();
        }
        let mut idle = serde_json::to_value(&self.idle.extensions)?;
        idle["screensaver"] = self.idle.screensaver.as_secs().into();
        idle["lock"] = self.idle.lock.as_secs().into();
        let mut config = serde_json::to_value(&self.extensions)?;
        config["version"] = 1.into();
        config["bar"] = bar;
        config["idle"] = idle;
        config["plugins"] = self
            .plugins
            .iter()
            .map(Native::compile)
            .collect::<Result<Vec<_>, _>>()?
            .into();
        Ok(CompiledShell {
            config,
            bar: WireBar {
                id: "shell".into(),
                monitor_id: String::new(),
                edge: match self.bar.position {
                    Position::Top => Edge::Top,
                    Position::Bottom => Edge::Bottom,
                    Position::Left => Edge::Left,
                    Position::Right => Edge::Right,
                } as i32,
                modules,
            },
        })
    }
    fn reserved(fields: &BTreeMap<String, Value>, names: &[&str]) -> Result<(), ShellError> {
        if let Some(name) = names.iter().find(|name| fields.contains_key(**name)) {
            return Err(ShellError::Invalid(format!(
                "extension collides with {name}"
            )));
        }
        Ok(())
    }
}
impl Native {
    fn compile(&self) -> Result<Value, ShellError> {
        if self.id.as_str() == "omega.view" {
            return Err(ShellError::Invalid(
                "use PluginWidget to declare omega.view instances".into(),
            ));
        }
        Shell::reserved(&self.options, &["id"])?;
        let mut value = serde_json::to_value(&self.options)?;
        value["id"] = self.id.as_str().into();
        Ok(value)
    }
}
