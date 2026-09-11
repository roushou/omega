use super::*;

/// Import preserves unsupported fields. Ambiguous Omega placements are refused.
impl Shell {
    pub fn from_omarchy(source: &str) -> Result<Self, ShellError> {
        let mut root: serde_json::Map<String, Value> =
            serde_json::from_str(source).map_err(|error| ShellError::Parse {
                input: source.into(),
                source: error,
            })?;
        if root.remove("version") != Some(Value::from(1)) {
            return Err(ShellError::Invalid(
                "only Omarchy shell version 1 is supported".into(),
            ));
        }
        let mut bar = Self::object(root.remove("bar"), "bar")?;
        let mut layout = Self::object(bar.remove("layout"), "bar.layout")?;
        let mut declared = Bar::top();
        declared.position =
            serde_json::from_value(bar.remove("position").unwrap_or(Value::from("top")))?;
        declared.transparent =
            serde_json::from_value(bar.remove("transparent").unwrap_or(Value::Bool(false)))?;
        declared.center_anchor = bar
            .remove("centerAnchor")
            .map(serde_json::from_value)
            .transpose()?;
        for (name, target) in [
            ("left", &mut declared.left),
            ("center", &mut declared.center),
            ("right", &mut declared.right),
        ] {
            let entries: Vec<Value> = serde_json::from_value(
                layout
                    .remove(name)
                    .unwrap_or_else(|| Value::Array(Vec::new())),
            )?;
            for entry in entries {
                let mut entry = Self::entry(entry)?;
                let id: String = serde_json::from_value(
                    entry
                        .remove("id")
                        .ok_or_else(|| ShellError::Invalid("widget has no id".into()))?,
                )?;
                if id == "omega.view" {
                    let unit: String =
                        serde_json::from_value(entry.remove("unit").ok_or_else(|| {
                            ShellError::Invalid("omega.view needs a unit before import".into())
                        })?)?;
                    let module: String = serde_json::from_value(
                        entry
                            .remove("module")
                            .unwrap_or_else(|| unit.clone().into()),
                    )?;
                    let mut plugin = PluginWidget::try_new(module, unit)?;
                    plugin.surface = serde_json::from_value(
                        entry.remove("surface").unwrap_or_else(|| "".into()),
                    )?;
                    plugin.panel = entry
                        .remove("panel")
                        .map(serde_json::from_value)
                        .transpose()?;
                    if !entry.is_empty() {
                        return Err(ShellError::Invalid(format!(
                            "unsupported omega.view options: {:?}",
                            entry.keys()
                        )));
                    }
                    target.push(plugin.into());
                } else {
                    target.push(Native::try_new(id)?.options(entry)?.into());
                }
            }
        }
        if !layout.is_empty() {
            return Err(ShellError::Invalid("unknown layout sections".into()));
        }
        declared.extensions = bar.into_iter().collect();
        let mut idle = Self::object(root.remove("idle"), "idle")?;
        let idle = Idle {
            screensaver: Duration::from_secs(serde_json::from_value(
                idle.remove("screensaver").unwrap_or(Value::from(150)),
            )?),
            lock: Duration::from_secs(serde_json::from_value(
                idle.remove("lock").unwrap_or(Value::from(300)),
            )?),
            extensions: idle.into_iter().collect(),
        };
        let plugins: Vec<Value> = serde_json::from_value(
            root.remove("plugins")
                .unwrap_or_else(|| Value::Array(Vec::new())),
        )?;
        let mut services = Vec::new();
        for plugin in plugins {
            let mut plugin = Self::entry(plugin)?;
            let id: String = serde_json::from_value(
                plugin
                    .remove("id")
                    .ok_or_else(|| ShellError::Invalid("service plugin needs id".into()))?,
            )?;
            services.push(Native::try_new(id)?.options(plugin)?);
        }
        let shell = Self {
            bar: declared,
            idle,
            plugins: services,
            extensions: root.into_iter().collect(),
        };
        shell.compile()?;
        Ok(shell)
    }

    fn entry(value: Value) -> Result<serde_json::Map<String, Value>, ShellError> {
        match value {
            Value::Object(entry) => Ok(entry),
            Value::String(id) => Ok(serde_json::Map::from_iter([("id".into(), id.into())])),
            _ => Err(ShellError::Invalid(
                "plugin entry must be an id or an object".into(),
            )),
        }
    }

    fn object(
        value: Option<Value>,
        name: &str,
    ) -> Result<serde_json::Map<String, Value>, ShellError> {
        match value {
            Some(Value::Object(object)) => Ok(object),
            None => Ok(serde_json::Map::new()),
            _ => Err(ShellError::Invalid(format!("{name} must be an object"))),
        }
    }
}
