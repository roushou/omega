use super::*;

impl Shell {
    /// Emit editable Rust builders, preserving unknown options through the JSON escape hatch.
    pub fn rust_source(&self) -> Result<String, ShellError> {
        self.compile()?;
        let mut imports = vec!["Shell", "Bar", "Position", "Idle", "ShellError"];
        let items: Vec<_> = self
            .bar
            .left
            .iter()
            .chain(&self.bar.center)
            .chain(&self.bar.right)
            .collect();
        if !self.plugins.is_empty() || items.iter().any(|item| matches!(item, BarItem::Native(_))) {
            imports.push("Native");
        }
        if items.iter().any(|item| matches!(item, BarItem::Plugin(_))) {
            imports.push("PluginWidget");
        }
        if !self.extensions.is_empty()
            || !self.bar.extensions.is_empty()
            || !self.idle.extensions.is_empty()
            || self.plugins.iter().any(|native| !native.options.is_empty())
            || items
                .iter()
                .any(|item| matches!(item, BarItem::Native(native) if !native.options.is_empty()))
        {
            imports.push("json");
        }
        let mut source = format!(
            "use omega_document::shell::{{{}}};\nuse std::time::Duration;\n\npub fn shell() -> Result<Shell, ShellError> {{\n    Ok(Shell::new()\n        .bar(Bar::top()",
            imports.join(", ")
        );
        source.push_str(&format!(
            ".position(Position::{:?}).transparent({})",
            self.bar.position, self.bar.transparent
        ));
        if let Some(anchor) = &self.bar.center_anchor {
            source.push_str(&format!(".center_anchor({anchor:?})"));
        }
        for (name, entries) in [
            ("left", &self.bar.left),
            ("center", &self.bar.center),
            ("right", &self.bar.right),
        ] {
            source.push_str(&format!("\n            .{name}([\n"));
            for entry in entries {
                let item = match entry {
                    BarItem::Native(native) => native.source(),
                    BarItem::Plugin(plugin) => {
                        let mut item = format!(
                            "PluginWidget::try_new({:?}, {:?})?",
                            plugin.id.as_str(),
                            plugin.unit.as_str()
                        );
                        if !plugin.surface.is_empty() {
                            item.push_str(&format!(".surface({:?})", plugin.surface));
                        }
                        if let Some(panel) = &plugin.panel {
                            item.push_str(&format!(".panel({panel:?})"));
                        }
                        if !plugin.settings.is_empty() {
                            return Err(ShellError::Invalid("Rust import requires typed placement settings to be supplied by the config author".into()));
                        }
                        item
                    }
                };
                source.push_str(&format!("                {item}.into(),\n"));
            }
            source.push_str("            ])");
        }
        for (key, value) in &self.bar.extensions {
            source.push_str(&format!(
                "\n            .extension({key:?}, json!({}))?",
                RustSource::value(value)
            ));
        }
        source.push_str(")\n");
        source.push_str(&format!("        .idle(Idle::new().screensaver_after(Duration::from_secs({})).lock_after(Duration::from_secs({}))",
            self.idle.screensaver.as_secs(), self.idle.lock.as_secs()));
        for (key, value) in &self.idle.extensions {
            source.push_str(&format!(
                ".extension({key:?}, json!({}))?",
                RustSource::value(value)
            ));
        }
        source.push_str(")\n");
        for plugin in &self.plugins {
            source.push_str(&format!("        .plugin({})\n", plugin.source()));
        }
        for (key, value) in &self.extensions {
            source.push_str(&format!(
                "        .extension({key:?}, json!({}))?\n",
                RustSource::value(value)
            ));
        }
        source.push_str("    )\n}\n");
        Ok(source)
    }
}
impl Native {
    fn source(&self) -> String {
        let mut source = format!("Native::new({:?})", self.id.as_str());
        if !self.options.is_empty() {
            let value = Value::Object(self.options.clone().into_iter().collect());
            source.push_str(&format!(".options(json!({}))?", RustSource::value(&value)));
        }
        source
    }
}
struct RustSource;
impl RustSource {
    fn value(value: &Value) -> String {
        match value {
            Value::Null => "null".into(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => value.to_string(),
            Value::String(value) => format!("{value:?}"),
            Value::Array(values) => format!(
                "[{}]",
                values
                    .iter()
                    .map(Self::value)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Value::Object(values) => format!(
                "{{{}}}",
                values
                    .iter()
                    .map(|(key, value)| format!("{key:?}: {}", Self::value(value)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}
