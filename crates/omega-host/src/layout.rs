//! The on-disk contract, declared once.
//!
//! Every path the CLI and daemon agree on — source layout, build output, and
//! the assembled state dir — is derived here from three roots. Callers never
//! join `units/<name>/unit.pb` by hand; they ask the [`Layout`], and for a
//! TOML document they ask [`Layout::file`], which hands back a typed
//! [`TomlFile`] located by the document's own schema.

use std::path::{Path, PathBuf};

use crate::toml::{TomlFile, TomlSchema};
use omega_proto::{Manifest, UnitName};

/// Which cargo profile a build produces, and so which directory its binaries
/// land in.
///
/// A release build is what a machine runs. A debug build is what a person
/// iterating on a unit waits for, and it is worth several times its own
/// weight in seconds — so which one to produce belongs to whoever is waiting.
/// The daemon does not care: a unit is a binary either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Profile {
    /// Fast to build, slow to run: the inner loop.
    Debug,
    /// What `omega build` produces and the daemon runs.
    #[default]
    Release,
}

impl Profile {
    /// The directory cargo puts this profile's output in.
    pub fn dir(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }

    /// The flag that asks cargo for it. Debug is cargo's default and is
    /// spelled by asking for nothing.
    pub fn flag(self) -> Option<&'static str> {
        match self {
            Self::Debug => None,
            Self::Release => Some("--release"),
        }
    }
}

/// Omega's filesystem layout: three roots plus every derived path.
#[derive(Debug, Clone)]
pub struct Layout {
    pub config: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
    pub shell_config: PathBuf,
}

impl Layout {
    /// The crate that emits the state document. Named once here because the
    /// CLI scaffolds it, cargo builds it, and the build runs it.
    pub const SYSTEM_CRATE: &'static str = "system";

    /// The listing of what a build produced, inside the state dir. The layout
    /// places the file; what goes in it is `omega-manifest`'s business.
    pub const UNITS_TOML: &'static str = "units.toml";

    /// Resolve the three roots (env-overridable, XDG defaults).
    pub fn resolve() -> Self {
        Self {
            shell_config: std::env::var_os("OMEGA_SHELL_CONFIG")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                        .join(".config/omarchy/shell.json")
                }),
            config: Self::resolve_dir("OMEGA_CONFIG_DIR", "XDG_CONFIG_HOME", ".config"),
            state: Self::resolve_dir("OMEGA_STATE_DIR", "XDG_STATE_HOME", ".local/state"),
            cache: Self::resolve_dir("OMEGA_CACHE_DIR", "XDG_CACHE_HOME", ".cache"),
        }
    }

    /// An explicit layout (tests, unusual deployments).
    pub fn at(
        config: impl Into<PathBuf>,
        state: impl Into<PathBuf>,
        cache: impl Into<PathBuf>,
    ) -> Self {
        let config = config.into();
        Self {
            shell_config: config.join("host/omarchy/shell.json"),
            config,
            state: state.into(),
            cache: cache.into(),
        }
    }

    /// The file holding one instance of a TOML document.
    ///
    /// The schema decides where it lives; this is the only way call sites
    /// name a document.
    pub fn file<S: TomlSchema>(&self, key: S::Key<'_>) -> TomlFile<S> {
        S::locate(self, key)
    }

    /// Select the external shell explicitly for isolated configurations.
    pub fn with_shell_config(mut self, path: impl Into<PathBuf>) -> Self {
        self.shell_config = path.into();
        self
    }

    pub fn compiled_shell(&self) -> PathBuf {
        self.state.join("shell.json")
    }
    pub fn shell_receipt(&self) -> PathBuf {
        self.generations_dir().join("shell-receipt.json")
    }
    pub fn shell_lock(&self) -> PathBuf {
        self.generations_dir().join("shell.lock")
    }
    pub fn shell_backup(&self) -> PathBuf {
        self.generations_dir().join("shell-before-omega.json")
    }
    pub fn shell_import(&self) -> PathBuf {
        self.system_dir().join("src/shell_import.rs")
    }

    // ---- source ----

    /// `~/.config/omega/Cargo.toml` — the workspace manifest.
    pub fn workspace_manifest(&self) -> PathBuf {
        self.config.join("Cargo.toml")
    }

    /// `~/.config/omega/.cargo/config.toml` — where this machine says the
    /// omega crates actually are.
    ///
    /// Not committed: it is the one file in a config that is about the
    /// machine rather than about the desktop.
    pub fn cargo_config(&self) -> PathBuf {
        self.config.join(".cargo").join("config.toml")
    }

    /// `~/.config/omega/.gitignore`.
    pub fn gitignore(&self) -> PathBuf {
        self.config.join(".gitignore")
    }

    /// `~/.config/omega/units`.
    pub fn units_dir(&self) -> PathBuf {
        self.config.join("units")
    }

    /// `~/.config/omega/system` — the configuration plane's crate.
    pub fn system_dir(&self) -> PathBuf {
        self.config.join(Self::SYSTEM_CRATE)
    }

    /// `~/.config/omega/system/Cargo.toml`.
    pub fn system_manifest(&self) -> PathBuf {
        self.system_dir().join("Cargo.toml")
    }

    /// `~/.config/omega/system/src/main.rs`.
    pub fn system_main(&self) -> PathBuf {
        self.system_dir().join("src").join("main.rs")
    }

    /// The compiled document emitter.
    pub fn compiled_system(&self, profile: Profile) -> PathBuf {
        self.profile_dir(profile).join(Self::SYSTEM_CRATE)
    }

    /// `~/.config/omega/units/<name>`.
    pub fn unit_src_dir(&self, name: &UnitName) -> PathBuf {
        self.units_dir().join(name.as_str())
    }

    /// `~/.config/omega/units/<name>/Cargo.toml`.
    pub fn unit_crate_manifest(&self, name: &UnitName) -> PathBuf {
        self.unit_src_dir(name).join("Cargo.toml")
    }

    /// `~/.config/omega/units/<name>/src/lib.rs` — the plugin itself.
    ///
    /// A plugin is a library as well as a program, so the config plane can
    /// depend on it and be checked against the settings it declares.
    pub fn unit_lib_src(&self, name: &UnitName) -> PathBuf {
        self.unit_src_dir(name).join("src").join("lib.rs")
    }

    /// `~/.config/omega/units/<name>/src/main.rs` — the program that runs it.
    pub fn unit_main_src(&self, name: &UnitName) -> PathBuf {
        self.unit_src_dir(name).join("src").join("main.rs")
    }

    // ---- build output ----

    /// `~/.config/omega/target` — the cargo target dir.
    ///
    /// Beside the source, which is where cargo puts one by default. That is
    /// the point: `omega build` passes this path explicitly and a person
    /// running `cargo build` in the config gets the same directory, so the
    /// two cannot each fill one. The watcher ignores it and the scaffold
    /// gitignores it.
    pub fn target_dir(&self) -> PathBuf {
        self.config.join("target")
    }

    /// `~/.config/omega/target/<profile>`.
    pub fn profile_dir(&self, profile: Profile) -> PathBuf {
        self.target_dir().join(profile.dir())
    }

    /// `~/.cache/omega/logs` — unit output.
    ///
    /// Logs outlive both build activation and generation reclamation.
    pub fn logs_dir(&self) -> PathBuf {
        self.cache.join("logs")
    }

    /// `~/.cache/omega/logs/<name>.log`.
    pub fn unit_log(&self, name: &UnitName) -> PathBuf {
        self.logs_dir().join(format!("{}.log", name.as_str()))
    }

    /// The compiled binary for a unit (cargo names it after the crate).
    pub fn compiled_binary(&self, profile: Profile, name: &UnitName) -> PathBuf {
        self.profile_dir(profile).join(name.as_str())
    }

    /// The mutable shell-sourceable session environment.
    pub fn environment(&self) -> PathBuf {
        self.state.join("environment")
    }

    /// Immutable build directories, retained while processes may reference them.
    pub fn generations_dir(&self) -> PathBuf {
        self.state.join("generations")
    }

    /// The atomically published generation name.
    pub fn active_build(&self) -> PathBuf {
        self.state.join("current")
    }

    /// Durable accepted and previous generation identifiers.
    pub fn generation_history(&self) -> PathBuf {
        self.state.join("generations.toml")
    }

    /// Serializes publication, acceptance, lease acquisition and cleanup.
    pub fn generation_lock(&self) -> PathBuf {
        self.state.join(".generations.lock")
    }

    /// A lease on this pinned generation directory.
    pub fn generation_lease(&self) -> PathBuf {
        self.state.join(".lease")
    }

    /// Marks a complete generation managed by the lease-aware store.
    pub fn generation_ready(&self) -> PathBuf {
        self.state.join(".ready")
    }

    // ---- assembled state ----

    /// `~/.local/state/omega/units`.
    pub fn state_units_dir(&self) -> PathBuf {
        self.state.join("units")
    }

    /// `~/.local/state/omega/units/<name>`.
    pub fn state_unit_dir(&self, name: &UnitName) -> PathBuf {
        self.state_units_dir().join(name.as_str())
    }

    /// `~/.local/state/omega/units/<name>/<name>` — the unit's binary.
    pub fn state_unit_program(&self, name: &UnitName) -> PathBuf {
        self.state_unit_dir(name).join(name.as_str())
    }

    /// `~/.local/state/omega/units/<name>/unit.pb` — the canonical manifest,
    /// as the exact bytes the plugin answered with and the daemon hashes.
    pub fn state_unit_manifest(&self, name: &UnitName) -> PathBuf {
        self.state_unit_dir(name).join(Manifest::FILE_NAME)
    }

    /// `~/.local/state/omega/units.toml` — the daemon's state config.
    pub fn state_units_toml(&self) -> PathBuf {
        self.state.join(Self::UNITS_TOML)
    }

    // ---- relative paths (the shape persisted in units.toml) ----

    /// `units/<name>/<name>`, relative to the state dir.
    pub fn unit_program_rel(&self, name: &UnitName) -> PathBuf {
        Path::new("units").join(name.as_str()).join(name.as_str())
    }

    /// `units/<name>/unit.pb`, relative to the state dir.
    pub fn unit_manifest_rel(&self, name: &UnitName) -> PathBuf {
        Path::new("units")
            .join(name.as_str())
            .join(Manifest::FILE_NAME)
    }

    fn resolve_dir(env_override: &str, xdg_env: &str, fallback: &str) -> PathBuf {
        std::env::var(env_override)
            .map(PathBuf::from)
            .or_else(|_| std::env::var(xdg_env).map(|d| PathBuf::from(d).join("omega")))
            .unwrap_or_else(|_| Self::home().join(fallback).join("omega"))
    }

    fn home() -> PathBuf {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
    }
}
