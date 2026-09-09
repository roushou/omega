//! Authoring a state document.
//!
//! The API a config's `system/` crate writes against. It exists for one
//! reason: the typed path has to be shorter than the untyped one, or the
//! config plane is just JSON with extra steps.

use omega_proto::omega::{
    Action, Bar, BatteryModule, ClockModule, CursorSetting, Edge, EnvironmentVariable, IdleSetting,
    InvokeUnit, Keybind, Modifier, Module, NightLightSetting, Notify, RunCommand, Schedule,
    Setting, StateDocument, ThemeSetting, UnitRef, WidgetModule, action, idle_setting, module,
    setting,
};

use crate::keys::Key;
use omega_proto::{Cadence, Fields, IntoValue, Values};

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

    /// Something the daemon does on its own clock. See [`Schedules`].
    pub fn schedule(mut self, schedule: Schedule) -> Self {
        self.inner.schedules.push(schedule);
        self
    }

    /// A key that does something. See [`Keybinds`].
    ///
    /// Nothing converges these yet — `omega build` refuses a document that
    /// declares one rather than staging a bind that would never fire.
    pub fn keybind(mut self, keybind: Keybind) -> Self {
        self.inner.keybinds.push(keybind);
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
                surface: String::new(),
                panel: String::new(),
            }),
        )
    }

    /// Which of the unit's surfaces this placement draws.
    ///
    /// Only needed for a unit with more than one: naming a unit has always
    /// meant its only widget surface, and a unit with several cannot be
    /// addressed by name alone.
    ///
    /// ```
    /// # use omega_document::Modules;
    /// Modules::surface(Modules::plain_widget("wifi", "wifi"), "indicator");
    /// ```
    pub fn surface(module: Module, surface: impl Into<String>) -> Module {
        Self::mapped(module, |widget| widget.surface = surface.into())
    }

    /// A second of the unit's surfaces, drawn as a popout anchored to this
    /// one — the shape of every panel in the bar.
    ///
    /// The bar draws the first; pressing it opens the second. What the popout
    /// contains is the unit's business, and a unit that renders nothing for it
    /// has a panel with nothing in it rather than a panel that will not open.
    ///
    /// ```
    /// # use omega_document::Modules;
    /// let wifi = Modules::plain_widget("wifi", "wifi");
    /// Modules::panel(Modules::surface(wifi, "indicator"), "details");
    /// ```
    pub fn panel(module: Module, panel: impl Into<String>) -> Module {
        Self::mapped(module, |widget| widget.panel = panel.into())
    }

    /// Change a widget placement, leaving anything else alone: a clock has no
    /// surfaces to name, and saying so is not an error worth failing a build
    /// over.
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

/// Keys that do things.
///
/// The key is a [`Key`] rather than a string: nobody remembers that page-up
/// is spelled `Prior`, and a bind that names it wrong is a bind that never
/// fires and never says so.
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

    /// The same, held down: a brightness key that keeps going while it is.
    pub fn repeating(keybind: Keybind) -> Keybind {
        Keybind {
            repeat: true,
            ..keybind
        }
    }
}

/// Work the machine does on its own.
///
/// The cadence belongs here rather than in a plugin, because how often the
/// weather is fetched is a property of the machine and the person using it —
/// not of the code that knows how to fetch it. A plugin author who picked
/// ten minutes would be picking it for everybody.
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
    ///     Actions::invoke("weather", "refresh"),
    /// );
    /// ```
    pub fn every(id: impl Into<String>, cadence: Cadence, action: Action) -> Schedule {
        Schedule::new(id, cadence, action)
    }

    /// Announce a cadence and leave what to do about it to whoever is
    /// listening — a unit with a reaction registered for
    /// [`EventKind::EventScheduleFired`], which tells schedules apart by id.
    ///
    /// [`EventKind::EventScheduleFired`]: omega_proto::omega::EventKind::EventScheduleFired
    pub fn announcing(id: impl Into<String>, cadence: Cadence) -> Schedule {
        Schedule::announcing(id, cadence)
    }
}

/// The things a schedule or a keybind can be told to do.
///
/// A thin builder over `action.proto`: the actions here are the ones a
/// document has a reason to name. The rest of the taxonomy exists for units
/// to ask for, where the capability check that governs it lives.
#[derive(Debug)]
pub struct Actions;

impl Actions {
    /// Call a unit's command surface. The daemon checks the unit declares it.
    pub fn invoke(unit: impl Into<String>, command: impl Into<String>) -> Action {
        Self::invoke_with(unit, command, Vec::<bool>::new())
    }

    /// The same, with arguments — the values the command reads with
    /// `Args::get`.
    pub fn invoke_with(
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

    /// Say something.
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
