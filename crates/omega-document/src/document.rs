//! Builders for the desired-state document emitted by `system/`.

use omega_proto::omega::{
    Action, Bar, BatteryModule, ClockModule, CursorSetting, Edge, EnvironmentVariable, IdleSetting,
    InvokeUnit, Keybind, Modifier, Module, NightLightSetting, Notify, RunCommand, Schedule,
    Setting, StateDocument, ThemeSetting, UnitRef, WidgetModule, action, idle_setting, module,
    setting,
};

use crate::keys::Key;
use omega_proto::{Cadence, Fields, IntoValue, Values};

/// Desktop desired state: plugin settings, presentations, schedules, and integrations.
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

    /// Construct an independent presentation once per plugin incarnation.
    pub fn presentation(
        mut self,
        presentation: impl Into<omega_proto::omega::ConfiguredPresentation>,
    ) -> Self {
        self.inner.presentations.push(presentation.into());
        self
    }

    pub fn bar(mut self, bar: Bar) -> Self {
        self.inner.bars.push(bar);
        self
    }

    /// Compose a typed integration into this document.
    pub fn with<E: crate::DocumentExtension>(mut self, extension: E) -> Result<Self, E::Error> {
        extension.apply(&mut self.inner)?;
        Ok(self)
    }

    pub fn setting(mut self, setting: Setting) -> Self {
        self.inner.settings.push(setting);
        self
    }

    /// Add a recurring action. See [`Schedules`].
    pub fn schedule(mut self, schedule: Schedule) -> Self {
        self.inner.schedules.push(schedule);
        self
    }

    /// Add a keybinding declaration. Core keybinding reconciliation is not supported;
    /// `omega build` rejects these declarations. Configure bindings through a supported host.
    pub fn keybind(mut self, keybind: Keybind) -> Self {
        self.inner.keybinds.push(keybind);
        self
    }

    /// Configure a built plugin. Unlisted plugins run with default settings.
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

    /// Write the document as JSON to stdout for `omega build`.
    /// The system entry point must not write other output to stdout.
    pub fn emit(self) -> crate::Result<()> {
        use std::io::Write;
        std::io::stdout()
            .lock()
            .write_all(crate::DocumentFile::encode(&self.inner)?.as_bytes())?;
        Ok(())
    }
}

/// Host information for conditional configuration.
#[derive(Debug)]
pub struct Host;

impl Host {
    /// Return the hostname, or `"unknown"` if it cannot be read.
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

    /// Create a bar widget placement with typed instance settings.
    pub fn widget(
        id: impl Into<String>,
        unit: impl Into<String>,
        settings: &impl Fields,
    ) -> Module {
        Self::configured(id, unit, settings.write())
    }

    /// Create a bar widget placement without instance settings.
    pub fn plain_widget(id: impl Into<String>, unit: impl Into<String>) -> Module {
        Self::configured(id, unit, Values::new())
    }

    /// Create a bar widget placement using raw settings values.
    pub fn configured(id: impl Into<String>, unit: impl Into<String>, settings: Values) -> Module {
        Self::of(
            id,
            module::Kind::Widget(WidgetModule {
                unit: unit.into(),
                config: settings.into_map(),
                surface: String::new(),
                panel: String::new(),
            }),
        )
    }

    /// Set the surface ID drawn by a widget placement.
    ///
    /// ```
    /// # use omega_document::Modules;
    /// Modules::surface(Modules::plain_widget("wifi", "wifi"), "indicator");
    /// ```
    pub fn surface(module: Module, surface: impl Into<String>) -> Module {
        Self::mapped(module, |widget| widget.surface = surface.into())
    }

    /// Attach a popup surface to a widget placement. Activating the widget opens it.
    ///
    /// ```
    /// # use omega_document::Modules;
    /// let wifi = Modules::plain_widget("wifi", "wifi");
    /// Modules::panel(Modules::surface(wifi, "indicator"), "details");
    /// ```
    pub fn panel(module: Module, panel: impl Into<String>) -> Module {
        Self::mapped(module, |widget| widget.panel = panel.into())
    }

    /// Modify widget placement fields; other module kinds are unchanged.
    fn mapped(mut module: Module, change: impl FnOnce(&mut WidgetModule)) -> Module {
        if let Some(module::Kind::Widget(widget)) = module.kind.as_mut() {
            change(widget);
        }
        module
    }

    fn of(id: impl Into<String>, kind: module::Kind) -> Module {
        Module {
            id: id.into(),
            kind: Some(kind),
        }
    }
}

/// Construct keybinding declarations using typed keyboard keys.
///
/// ```
/// # use omega_document::{Actions, Key, Keybinds};
/// # use omega_proto::omega::Modifier;
/// Keybinds::on("lock", Key::L, [Modifier::Super], Actions::run("omarchy-lock-screen"));
/// ```
#[derive(Debug)]
pub struct Keybinds;

impl Keybinds {
    pub fn on(
        id: impl Into<String>,
        key: Key,
        modifiers: impl IntoIterator<Item = Modifier>,
        action: Action,
    ) -> Keybind {
        Keybind {
            id: id.into(),
            modifiers: modifiers.into_iter().map(|held| held as i32).collect(),
            key: key.keysym().to_string(),
            action: Some(action),
            repeat: false,
        }
    }

    /// Enable repeated activation while the key is held.
    pub fn repeating(keybind: Keybind) -> Keybind {
        Keybind {
            repeat: true,
            ..keybind
        }
    }
}

/// Recurring actions and schedule events configured by the system document.
#[derive(Debug)]
pub struct Schedules;

impl Schedules {
    /// Fire an action on a cadence.
    ///
    /// ```
    /// # use omega_document::{Actions, Schedules};
    /// # use omega_proto::Cadence;
    /// Schedules::every(
    ///     "refresh-weather",
    ///     Cadence::minutes(10),
    ///     Actions::invoke_named("weather", "refresh"),
    /// );
    /// ```
    pub fn every(id: impl Into<String>, cadence: Cadence, action: Action) -> Schedule {
        Schedule::new(id, cadence, action)
    }

    /// Emit [`EventKind::EventScheduleFired`] at each interval without an action.
    /// Reactions can distinguish schedules by ID.
    ///
    /// [`EventKind::EventScheduleFired`]: omega_proto::omega::EventKind::EventScheduleFired
    pub fn announcing(id: impl Into<String>, cadence: Cadence) -> Schedule {
        Schedule::announcing(id, cadence)
    }
}

/// Actions for schedules and keybindings.
#[derive(Debug)]
pub struct Actions;

impl Actions {
    /// Invoke a command that takes no input. Registration is checked by the daemon.
    ///
    /// ```
    /// use omega::Command;
    /// use omega_document::Actions;
    /// #[derive(omega::Command)]
    /// struct Refresh {}
    /// impl Command for Refresh {
    ///     type Input = ();
    ///     type Output = ();
    ///     async fn call(&self, _: ()) -> omega::Result<()> { Ok(()) }
    /// }
    /// let action = Actions::invoke(Refresh);
    /// ```
    /// Commands requiring input cannot be invoked without it:
    ///
    /// ```compile_fail
    /// use omega::Command;
    /// #[derive(omega::Command)]
    /// struct SetLevel;
    /// impl Command for SetLevel {
    ///     type Input = u64;
    ///     type Output = ();
    ///     async fn call(&self, _: u64) -> omega::Result<()> { Ok(()) }
    /// }
    /// omega_document::Actions::invoke(SetLevel);
    /// ```
    pub fn invoke<C: ::omega::Command<Input = ()>>(
        command: impl Into<::omega::command::CommandRef<C>>,
    ) -> Action {
        Self::invoke_with(command, ())
    }

    /// Invoke a command with its complete, typed input.
    ///
    /// ```
    /// use omega::Command;
    /// use omega_document::Actions;
    /// #[derive(omega::Command)]
    /// struct SetLevel {}
    /// impl Command for SetLevel {
    ///     type Input = u64;
    ///     type Output = ();
    ///     async fn call(&self, _: u64) -> omega::Result<()> { Ok(()) }
    /// }
    /// let action = Actions::invoke_with(SetLevel, 42);
    /// ```
    /// The input must match the command's declared type:
    ///
    /// ```compile_fail
    /// use omega::Command;
    /// #[derive(omega::Command)]
    /// struct SetLevel;
    /// impl Command for SetLevel {
    ///     type Input = u64;
    ///     type Output = ();
    ///     async fn call(&self, _: u64) -> omega::Result<()> { Ok(()) }
    /// }
    /// omega_document::Actions::invoke_with(SetLevel, "loud");
    /// ```
    pub fn invoke_with<C: ::omega::Command>(
        command: impl Into<::omega::command::CommandRef<C>>,
        input: C::Input,
    ) -> Action {
        use ::omega::Input;
        let command = command.into();
        Self::invoke_named_with(command.unit(), command.name(), input.encode())
    }

    /// Invoke a dynamically named command without input. Prefer [`Self::invoke`]
    /// when the defining plugin is a dependency.
    pub fn invoke_named(unit: impl Into<String>, command: impl Into<String>) -> Action {
        Self::invoke_named_with(unit, command, Vec::<bool>::new())
    }

    /// Invoke a dynamically named command with positional wire values.
    /// The daemon validates the target and input against the plugin manifest.
    pub fn invoke_named_with(
        unit: impl Into<String>,
        command: impl Into<String>,
        args: impl IntoIterator<Item = impl IntoValue>,
    ) -> Action {
        Self::of(action::Kind::InvokeUnit(InvokeUnit {
            unit: unit.into(),
            command: command.into(),
            args: args.into_iter().map(IntoValue::into_value).collect(),
        }))
    }

    /// Run a shell command. The escape hatch, and honestly so.
    pub fn run(command: impl Into<String>) -> Action {
        Self::of(action::Kind::RunCommand(RunCommand {
            command: command.into(),
        }))
    }

    /// Create a desktop notification action.
    pub fn notify(summary: impl Into<String>, body: impl Into<String>) -> Action {
        Self::of(action::Kind::Notify(Notify {
            summary: summary.into(),
            body: body.into(),
            icon: String::new(),
            timeout_ms: 0,
        }))
    }

    fn of(kind: action::Kind) -> Action {
        Action { kind: Some(kind) }
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

    /// Enable a plugin explicitly. Built plugins are enabled by default.
    pub fn enabled(name: impl Into<String>) -> UnitRef {
        UnitRef {
            name: name.into(),
            enabled: true,
            config: Default::default(),
        }
    }

    /// Enable a plugin with typed unit settings.
    /// Settings apply to all its commands, reactions, and surfaces. Placement settings
    /// override only the keys they provide. Changing unit settings restarts the plugin.
    pub fn configured(name: impl Into<String>, settings: &impl Fields) -> UnitRef {
        UnitRef {
            name: name.into(),
            enabled: true,
            config: settings.write().into_map(),
        }
    }
}
