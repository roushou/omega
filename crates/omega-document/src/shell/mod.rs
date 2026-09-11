//! Declarative Omarchy configuration. A shell owns the entire generated file.
//!
//! ```
//! use omega_document::shell::{Shell, Bar, Native, PluginWidget, Idle};
//! use std::time::Duration;
//! let shell = Shell::new()
//!     .bar(Bar::top()
//!         .left([Native::menu().into()])
//!         .center([Native::clock().into()])
//!         .right([PluginWidget::new("audio", "audio")
//!             .surface("indicator").panel("panel").into()]))
//!     .idle(Idle::new().lock_after(Duration::from_secs(300)));
//! let document = omega_document::Document::new().shell(shell)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod compiler;
mod import;
mod native;
mod source;
pub use native::{Clock, NativeId};
pub use serde_json::json;
#[cfg(test)]
mod tests;

use omega_proto::Fields;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

pub use compiler::CompiledShell;

/// Invalid shell declarations fail before a generation is published.
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("invalid shell configuration: {0}")]
    Invalid(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// One complete shell declaration; omitted settings use Omarchy's own defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shell {
    bar: Bar,
    idle: Idle,
    plugins: Vec<Native>,
    extensions: BTreeMap<String, Value>,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            bar: Bar::top(),
            idle: Idle::new(),
            plugins: Vec::new(),
            extensions: BTreeMap::new(),
        }
    }
}

impl Shell {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn bar(mut self, bar: Bar) -> Self {
        self.bar = bar;
        self
    }
    pub fn idle(mut self, idle: Idle) -> Self {
        self.idle = idle;
        self
    }
    pub fn plugin(mut self, plugin: Native) -> Self {
        self.plugins.push(plugin);
        self
    }
    /// Preserve an unsupported top-level setting without overriding typed settings.
    pub fn extension(
        mut self,
        name: impl Into<String>,
        value: impl Serialize,
    ) -> Result<Self, ShellError> {
        let name = name.into();
        if ["version", "bar", "idle", "plugins"].contains(&name.as_str()) {
            return Err(ShellError::Invalid(format!("{name} has a typed API")));
        }
        self.extensions.insert(name, serde_json::to_value(value)?);
        Ok(self)
    }
    pub(crate) fn encode(&self) -> Result<String, ShellError> {
        Ok(serde_json::to_string(self)?)
    }
    pub(crate) fn decode(source: &str) -> Result<Self, ShellError> {
        Ok(serde_json::from_str(source)?)
    }
}

/// Omarchy supports one bar layout, repeated by the shell across its displays.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bar {
    position: Position,
    transparent: bool,
    center_anchor: Option<String>,
    left: Vec<BarItem>,
    center: Vec<BarItem>,
    right: Vec<BarItem>,
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Top,
    Bottom,
    Left,
    Right,
}

impl Bar {
    pub fn top() -> Self {
        Self {
            position: Position::Top,
            transparent: false,
            center_anchor: None,
            left: Vec::new(),
            center: Vec::new(),
            right: Vec::new(),
            extensions: BTreeMap::new(),
        }
    }
    pub fn position(mut self, position: Position) -> Self {
        self.position = position;
        self
    }
    pub fn transparent(mut self, value: bool) -> Self {
        self.transparent = value;
        self
    }
    pub fn center_anchor(mut self, id: impl Into<String>) -> Self {
        self.center_anchor = Some(id.into());
        self
    }
    pub fn left(mut self, items: impl IntoIterator<Item = BarItem>) -> Self {
        self.left = items.into_iter().collect();
        self
    }
    pub fn center(mut self, items: impl IntoIterator<Item = BarItem>) -> Self {
        self.center = items.into_iter().collect();
        self
    }
    pub fn right(mut self, items: impl IntoIterator<Item = BarItem>) -> Self {
        self.right = items.into_iter().collect();
        self
    }
    pub fn extension(
        mut self,
        name: impl Into<String>,
        value: impl Serialize,
    ) -> Result<Self, ShellError> {
        let name = name.into();
        if ["position", "transparent", "centerAnchor", "layout"].contains(&name.as_str()) {
            return Err(ShellError::Invalid(format!("{name} has a typed API")));
        }
        self.extensions.insert(name, serde_json::to_value(value)?);
        Ok(self)
    }
}

/// Native and Omega widgets share one ordered layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum BarItem {
    Native(Native),
    Plugin(PluginWidget),
}
impl From<Native> for BarItem {
    fn from(value: Native) -> Self {
        Self::Native(value)
    }
}
impl From<PluginWidget> for BarItem {
    fn from(value: PluginWidget) -> Self {
        Self::Plugin(value)
    }
}

/// A native Omarchy widget or service; options belong to that plugin's schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Native {
    id: NativeId,
    options: BTreeMap<String, Value>,
}
impl Native {
    pub fn new(id: impl Into<String>) -> Self {
        Self::try_new(id).expect("valid native plugin id")
    }
    pub fn try_new(id: impl Into<String>) -> Result<Self, ShellError> {
        Ok(Self {
            id: NativeId::parse(id)?,
            options: BTreeMap::new(),
        })
    }
    pub fn menu() -> Self {
        Self::new("omarchy.menu")
    }
    pub fn workspaces() -> Self {
        Self::new("omarchy.workspaces")
    }
    pub fn clock() -> Clock {
        Clock::new()
    }
    pub fn tray() -> Self {
        Self::new("omarchy.tray")
    }
    pub fn power() -> Self {
        Self::new("omarchy.power")
    }
    /// Options must serialize as an object. The plugin identity cannot be overwritten.
    pub fn options(mut self, options: impl Serialize) -> Result<Self, ShellError> {
        let value = serde_json::to_value(options)?;
        let Value::Object(options) = value else {
            return Err(ShellError::Invalid(
                "plugin options must be an object".into(),
            ));
        };
        if options.contains_key("id") {
            return Err(ShellError::Invalid(
                "plugin options cannot replace id".into(),
            ));
        }
        self.options.extend(options);
        Ok(self)
    }
}

/// A plugin placement is also the sole declaration of its render instances.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginWidget {
    id: omega_proto::ModuleId,
    unit: omega_proto::UnitName,
    surface: String,
    panel: Option<String>,
    settings: BTreeMap<String, omega_proto::omega::Value>,
}
impl PluginWidget {
    /// Parse placement and unit identities at the authoring boundary.
    pub fn try_new(id: impl Into<String>, unit: impl Into<String>) -> Result<Self, ShellError> {
        Ok(Self {
            id: omega_proto::ModuleId::parse(id.into())
                .map_err(|e| ShellError::Invalid(e.to_string()))?,
            unit: omega_proto::UnitName::parse(unit.into())
                .map_err(|e| ShellError::Invalid(e.to_string()))?,
            surface: String::new(),
            panel: None,
            settings: BTreeMap::new(),
        })
    }
    /// For literal names. Dynamic names should use `try_new`.
    pub fn new(id: &str, unit: &str) -> Self {
        Self::try_new(id, unit).expect("valid placement and unit names")
    }
    pub fn surface(mut self, surface: impl Into<String>) -> Self {
        self.surface = surface.into();
        self
    }
    pub fn panel(mut self, panel: impl Into<String>) -> Self {
        self.panel = Some(panel.into());
        self
    }
    pub fn settings(mut self, settings: &impl Fields) -> Self {
        self.settings = settings.write().into_map().into_iter().collect();
        self
    }
}

/// Inactivity deadlines. Zero disables a deadline in Omarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Idle {
    screensaver: Duration,
    lock: Duration,
    extensions: BTreeMap<String, Value>,
}
impl Default for Idle {
    fn default() -> Self {
        Self {
            screensaver: Duration::from_secs(150),
            lock: Duration::from_secs(300),
            extensions: BTreeMap::new(),
        }
    }
}
impl Idle {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn screensaver_after(mut self, duration: Duration) -> Self {
        self.screensaver = duration;
        self
    }
    pub fn lock_after(mut self, duration: Duration) -> Self {
        self.lock = duration;
        self
    }
    pub fn extension(
        mut self,
        name: impl Into<String>,
        value: impl Serialize,
    ) -> Result<Self, ShellError> {
        let name = name.into();
        if ["screensaver", "lock"].contains(&name.as_str()) {
            return Err(ShellError::Invalid(format!("{name} has a typed API")));
        }
        self.extensions.insert(name, serde_json::to_value(value)?);
        Ok(self)
    }
}
