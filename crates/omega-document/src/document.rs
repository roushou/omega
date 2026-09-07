//! Authoring a state document.
//!
//! The API a config's `system/` crate writes against. It exists for one
//! reason: the typed path has to be shorter than the untyped one, or the
//! config plane is just JSON with extra steps.

use omega_proto::omega::{
    Bar, BatteryModule, ClockModule, CursorSetting, Edge, EnvironmentVariable, IdleSetting, Module,
    NightLightSetting, Setting, StateDocument, ThemeSetting, UnitRef, WidgetModule, idle_setting,
    module, setting,
};
use omega_proto::{Fields, Values};

/// The machine's desired state, built one declaration at a time.
///
/// Every method takes and returns `self`, so a config reads as one
/// expression: what the machine *is*, with no order of operations to get
/// wrong.
#[derive(Debug, Default, Clone)]
pub struct Document {
    inner: StateDocument,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stamp the document with the revision of the config that produced it.
    pub fn revision(mut self, revision: u64) -> Self {
        self.inner.revision = revision;
        self
    }

    pub fn bar(mut self, bar: Bar) -> Self {
        self.inner.bars.push(bar);
        self
    }

    pub fn setting(mut self, setting: Setting) -> Self {
        self.inner.settings.push(setting);
        self
    }

    /// Configure a unit the workspace builds. A unit that is built and not
    /// mentioned here runs as it is; mentioning it is how you turn it off or
    /// hand it configuration.
    pub fn unit(mut self, unit: UnitRef) -> Self {
        self.inner.units.push(unit);
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.inner.environment.push(EnvironmentVariable {
            key: key.into(),
            value: value.into(),
        });
        self
    }

    pub fn into_inner(self) -> StateDocument {
        self.inner
    }

    /// Print the document to stdout, which is how `omega build` collects it.
    ///
    /// A `system/` crate is one entry point with no side effects: it computes
    /// a document and says it. Anything else it writes to stdout would be
    /// part of the document, so there is nothing else to write.
    pub fn emit(self) -> Result<(), crate::DocumentError> {
        print!("{}", crate::DocumentFile::encode(&self.inner)?);
        Ok(())
    }
}

/// Which machine this is, for a config composed per host.
#[derive(Debug)]
pub struct Host;

impl Host {
    /// The machine's hostname, or `"unknown"` where it cannot be read. A
    /// config branches on this rather than on a git branch.
    pub fn name() -> String {
        std::fs::read_to_string("/etc/hostname")
            .map(|name| name.trim().to_string())
            .ok()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "unknown".to_string())
    }
}

/// Bars, by the edge they sit on.
#[derive(Debug)]
pub struct Bars;

impl Bars {
    pub fn top(id: impl Into<String>, modules: Vec<Module>) -> Bar {
        Self::at(id, Edge::Top, modules)
    }

    pub fn bottom(id: impl Into<String>, modules: Vec<Module>) -> Bar {
        Self::at(id, Edge::Bottom, modules)
    }

    pub fn at(id: impl Into<String>, edge: Edge, modules: Vec<Module>) -> Bar {
        Bar {
            id: id.into(),
            monitor_id: String::new(),
            edge: edge as i32,
            modules,
        }
    }

    /// Pin a bar to one monitor.
    pub fn on_monitor(mut bar: Bar, monitor_id: impl Into<String>) -> Bar {
        bar.monitor_id = monitor_id.into();
        bar
    }
}

/// The modules a bar is made of.
#[derive(Debug)]
pub struct Modules;

impl Modules {
    pub fn clock(id: impl Into<String>, format: impl Into<String>) -> Module {
        Self::of(
            id,
            module::Kind::Clock(ClockModule {
                format: format.into(),
            }),
        )
    }

    pub fn battery(id: impl Into<String>, show_percent: bool) -> Module {
        Self::of(id, module::Kind::Battery(BatteryModule { show_percent }))
    }

    /// A widget unit, instantiated in the bar with its own settings.
    ///
    /// The settings are the plugin's own type. A config workspace is one
    /// cargo workspace, so `system/` can depend on the plugin it configures
    /// and hand it a value the compiler has already checked — a misspelled
    /// setting is a build error here rather than a default silently taken on
    /// the machine.
    pub fn widget(
        id: impl Into<String>,
        unit: impl Into<String>,
        settings: &impl Fields,
    ) -> Module {
        Self::configured(id, unit, settings.write())
    }

    /// The same, for a widget that takes no settings.
    pub fn plain_widget(id: impl Into<String>, unit: impl Into<String>) -> Module {
        Self::configured(id, unit, Values::new())
    }

    /// The same, from values assembled by hand — for a setting whose type is
    /// not available here.
    pub fn configured(id: impl Into<String>, unit: impl Into<String>, settings: Values) -> Module {
        Self::of(
            id,
            module::Kind::Widget(WidgetModule {
                unit: unit.into(),
                config: settings.into_map(),
            }),
        )
    }

    fn of(id: impl Into<String>, kind: module::Kind) -> Module {
        Module {
            id: id.into(),
            kind: Some(kind),
        }
    }
}

/// System settings.
#[derive(Debug)]
pub struct Settings;

impl Settings {
    pub fn night_light(id: impl Into<String>, temperature_k: u32) -> Setting {
        Self::of(
            id,
            setting::Kind::NightLight(NightLightSetting {
                enabled: true,
                temperature_k,
            }),
        )
    }

    pub fn idle_lock(id: impl Into<String>, timeout_sec: u32) -> Setting {
        Self::of(
            id,
            setting::Kind::Idle(IdleSetting {
                timeout_sec,
                on_idle: Some(idle_setting::OnIdle::Lock(Default::default())),
            }),
        )
    }

    pub fn theme(id: impl Into<String>, name: impl Into<String>) -> Setting {
        Self::of(
            id,
            setting::Kind::Theme(ThemeSetting {
                name: name.into(),
                overrides: Default::default(),
            }),
        )
    }

    pub fn cursor(id: impl Into<String>, theme: impl Into<String>, size: u32) -> Setting {
        Self::of(
            id,
            setting::Kind::Cursor(CursorSetting {
                theme: theme.into(),
                size,
            }),
        )
    }

    fn of(id: impl Into<String>, kind: setting::Kind) -> Setting {
        Setting {
            id: id.into(),
            kind: Some(kind),
        }
    }
}

/// Units, as the document refers to them.
#[derive(Debug)]
pub struct Units;

impl Units {
    /// Turn a built unit off without deleting its crate.
    pub fn disabled(name: impl Into<String>) -> UnitRef {
        UnitRef {
            name: name.into(),
            enabled: false,
            config: Default::default(),
        }
    }

    /// Keep a unit on. A unit the document never mentions runs anyway; this
    /// is how to say so out loud.
    pub fn enabled(name: impl Into<String>) -> UnitRef {
        UnitRef {
            name: name.into(),
            enabled: true,
            config: Default::default(),
        }
    }

    /// Keep a unit on, and hand it settings — its own type, checked here.
    ///
    /// These are the unit's settings rather than one instance's. Every
    /// surface it offers is built out of them, which for a command or a
    /// reaction is the only configuration there is: neither is ever placed
    /// anywhere to be configured there.
    ///
    /// A widget placed in a bar is *also* configured where it is placed, and
    /// the two layer — the placement's settings over these — so configuring a
    /// plugin once is not repeated at every placement, and naming one key in
    /// a placement does not reset the rest.
    ///
    /// They reach the unit at its handshake, because that is when its fields
    /// are built out of them. Changing them therefore runs the unit again.
    pub fn configured(name: impl Into<String>, settings: &impl Fields) -> UnitRef {
        UnitRef {
            name: name.into(),
            enabled: true,
            config: settings.write().into_map(),
        }
    }
}
