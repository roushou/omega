# Omega

[![CI](https://github.com/roushou/omega/actions/workflows/ci.yml/badge.svg)](https://github.com/roushou/omega/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/omega-rs.svg)](https://crates.io/crates/omega-rs)
[![docs.rs](https://img.shields.io/docsrs/omega-rs)](https://docs.rs/omega-rs)
[![license](https://img.shields.io/crates/l/omega-rs.svg)](https://github.com/roushou/omega/blob/main/LICENSE)

_A declarative Rust framework and plugin runtime for [Omarchy](https://omarchy.org)._

Omega lets you write plugins, compose your bar, and build interactive interfaces in Rust.
Your desktop configuration is an ordinary Cargo workspace, and plugins are compiled to binaries.

No QML, JavaScript, or shell scripts.

## Why

Why write plugins fully in Rust when Omarchy already provides a native way to write them?

Several reasons:

- **One language.** An Omarchy plugin is a `manifest.json`, a QML component,
  JavaScript, and often a shell script for system access. An Omega plugin is a
  single Rust crate.

- **Typed API.** Readings, commands, and settings are declared as typed fields.
  A wrong property name or bad argument fails at `cargo build` instead of
  turning up in the shell console after a reload.

- **Just a Rust workspace.** Your shell layout and every plugin are ordinary
  crates in one workspace, kept in Git as a single repo. Your desktop diffs,
  branches, and merges like any other Rust project, and shared code is a crate
  dependency rather than a copy.

- **Testable.** Plugins are ordinary crates, so `cargo test`, Clippy, and CI
  apply to them as they do to any other crate.

- **The Rust ecosystem.** Parsing, networking, async, and serialization come
  from the battle-tested Rust ecosystem instead of being hand-rolled in JavaScript.

- **Process isolation.** Omarchy plugins run unsandboxed inside the one
  `omarchy-shell` process. Each Omega plugin is its own process, supervised by
  the daemon, so a failure does not take the shell with it.

## How it works

A configuration is a Cargo workspace with two kinds of members:

- `system/` computes the desired desktop from ordinary Rust code: which plugins
  run, where widgets are placed, and what settings they receive. It has no side
  effects.
- `plugins/` are independently runnable features, each compiled to its own binary.

The Omega daemon runs those binaries, shares system readings, routes commands and
UI interactions, and keeps the running desktop in line with what `system/`
declares. Plugins run as your user in separate supervised processes.

## Features

- Compose bar widgets, popup panels, and standalone windows and overlays.
- Build reusable UI components and preview them with sample data.
- Read system state and control devices through typed Rust APIs.
- Register typed commands, call other plugins, react to events, and schedule
  background work.
- Keep the configuration in Git as an ordinary Cargo workspace, with shared
  crates, tests, Clippy, and CI.
- Compile each plugin into a separate executable supervised by the daemon.

## Requirements

- Linux running Omarchy Quattro
- A systemd user manager
- Rust 1.88 or newer

## Installation

Install the [Omega CLI](https://crates.io/crates/omega-cli):

```sh
cargo install omega-cli --locked
```

Prebuilt CLI binaries are also available on the
[GitHub Releases page](https://github.com/roushou/omega/releases). Rust is still
required to build your configuration and plugins.

## Writing your first plugin

Initialize Omega. This creates a workspace under `~/.config/omega`, adopts and
backs up your existing shell configuration, installs the renderer and daemon
service, and builds and activates the configuration. It also restarts the shell.
The command reports backup and recovery locations.

Use `omega init --bare` if you only want to create the workspace files; the
walkthrough below uses the full setup.

```sh
omega init
cd ~/.config/omega
```

Scaffold a plugin named `audio`. This starts with the default greeting template;
we will replace it with an audio indicator and panel.

```sh
omega new audio
```

Your configuration should look like this:

```text
~/.config/omega/
├── Cargo.toml                 # Workspace members and shared dependencies
├── Cargo.lock                 # Resolved dependency versions
├── system/
│   ├── Cargo.toml             # Depends on the plugins you configure
│   └── src/
│       ├── main.rs            # Shell layout, plugin settings, and automations
│       └── shell_import.rs    # Rust layout generated when adopting the shell
└── plugins/
    └── audio/
        ├── Cargo.toml         # Plugin dependencies
        └── src/
            ├── lib.rs         # Surfaces, commands, settings, and plugin registration
            └── main.rs        # Runs the plugin as a separate process
```

Each crate under `plugins/` implements a plugin and `system/` is where you compose your Omarchy shell.

Replace `plugins/audio/src/lib.rs` with the following. The indicator shows the
volume in the bar; its panel contains a slider:

```rust
use omega::platform::audio::{Audio, Volume};
use omega::ui::{Section, Slider, Text};
use omega::{Command, Percent, Plugin, Surface, View};

#[derive(omega::Surface)]
pub struct Indicator {
    audio: Audio,
}

impl Surface for Indicator {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();

    fn update(&self, _: &mut (), message: Self::Message, _: &()) -> omega::surface::Task<Self::Message> {
        match message {}
    }

    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View {
        if !self.audio.has_reading() {
            return Text::new("Audio unavailable").into();
        }

        Text::new(self.audio.volume()).into()
    }
}

#[derive(omega::Surface)]
pub struct Panel {
    audio: Audio,
}

impl Surface for Panel {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();

    fn update(&self, _: &mut (), message: Self::Message, _: &()) -> omega::surface::Task<Self::Message> {
        match message {}
    }

    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View {
        if !self.audio.has_reading() {
            return Text::new("Audio unavailable").into();
        }

        Section::new("Audio")
            .child(Text::new(self.audio.volume()))
            .child(Slider::new(self.audio.volume()).on_change(SetVolume))
            .into()
    }
}

#[derive(omega::Command)]
struct SetVolume {
    volume: Volume,
}

impl Command for SetVolume {
    type Input = Percent;
    type Output = ();

    async fn call(&self, level: Percent) -> omega::Result<()> {
        self.volume.set(level).await
    }
}

pub fn plugin() -> Plugin {
    omega::plugin!()
        .surface(Indicator)
        .surface(Panel)
        .command::<SetVolume>()
}
```

Keep the generated `plugins/audio/src/main.rs`; it calls `audio::plugin().run()`.

Now place the indicator and attach its panel. Add this expression to a
`Bar::left(...)`, `Bar::center(...)`, or `Bar::right(...)` list in your system
layout, alongside the existing entries:

```rust
omega_omarchy::shell::PluginWidget::new("audio", audio::Indicator)
    .panel(audio::Panel)
    .into(),
```

Start at `system/src/main.rs`. When initialization imports your existing layout,
the bar declarations live in `system/src/shell_import.rs` instead. `omega new`
adds the Cargo dependency but leaves placement to you.

Build and activate the configuration:

```sh
omega build --wait
```

The volume indicator should now appear in the bar. Click it to open the slider.
Check plugin health with:

```sh
omega status
```

My personal configuration is available at [omx](https://github.com/roushou/omx) with examples of a launcher, calendar, device controls, and background data collection.

## Documentation

- [Plugin and component guide](docs/authoring.md): readings, commands, composition, and state.
- [API reference](https://docs.rs/omega-rs): types, methods, and examples.
- [Previews](docs/previews.md): fixtures, interaction, and visual comparisons.
- [Shared storage](docs/storage.md): typed stores, subscriptions, JSON persistence, and inspection (unreleased).
- [Keyboard input](docs/keyboard.md): shortcuts, scoped routing, and list navigation.
- [CLI guide](crates/omega-cli/README.md): build, inspect, and develop a config.

For maintainers: [development setup](docs/development.md), [architecture](docs/architecture.md),
[architectural principles](docs/principles.md),
[design decisions](docs/desktop-platform.md), [open questions](docs/design.md),
[contribution rules](AGENTS.md), and [release process](docs/releasing.md).

## License

[MIT](LICENSE)
