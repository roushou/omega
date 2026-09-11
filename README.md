# Omega

_A typed, declarative way to extend Omarchy in Rust._

Omega lets you write [Omarchy](https://omarchy.org)’s widgets, controls, and automations in Rust.
Omarchy provides the desktop and shell; Omega connects your plugins to system state,
runs them as separate processes, and renders their declarative interfaces through that shell.

Your configuration is an ordinary Cargo workspace.
Plugins are Rust crates, settings and command bindings are typed,
and reusable behavior can live in libraries.

Each plugin runs as its own binary, so a plugin crash does not bring down the shell.
Omega supervises those processes and brokers shared system integrations,
while your code describes what to display and what interactions should do.

## Running Omega

Omega runs as a background daemon. Your configuration lives in `~/.config/omega`,
a Rust workspace containing your plugins and the declaration of where they belong.

### Requirements

- Linux with a systemd user manager
- Omarchy
- Rust 1.88+

Install the [Omega CLI from crates.io](https://crates.io/crates/omega-cli):

```sh
cargo install omega-cli --locked
omega init
```

`omega init` creates the workspace, installs the renderer, and starts the daemon
as a user service.

You can inspect the service and its plugins from the terminal:

```sh
omega daemon status
omega status
```

For foreground operation, `omega daemon` runs the daemon directly.

## Writing plugins

A plugin declares its dependencies through its fields. Holding `Audio` gives a
widget access to the current audio state; holding `Volume` lets a command change
it. Omega derives the required subscriptions and permissions from those types.

Views are declarative, and controls bind directly to typed commands:

```rust
use omega::audio::{Audio, Volume};
use omega::ui::{Section, Slider, Text};
use omega::{Command, Percent, Ui, Widget};

#[derive(omega::Widget)]
struct Panel {
    audio: Audio,
}

impl Widget for Panel {
    fn render(&self) -> Ui {
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

fn main() -> omega::Result<()> {
    omega::plugin!()
        .widget::<Panel>()
        .command::<SetVolume>()
        .run()
}
```

Omega redraws the widget when its audio state changes. Moving the slider invokes
`SetVolume` with a `Percent`. Widgets can read state; actions belong in commands
or reactions, a distinction enforced at compile time.

`omega new` creates a plugin crate and prints the declaration for placing its
widget in `~/.config/omega/system/src/main.rs`. Build once to apply your changes,
or keep a build running while you work:

```sh
omega new audio
omega build
omega build --watch --debug
```

Plugins are ordinary Cargo projects, so their tests run with `cargo test` from
your configuration workspace. For development against the live desktop,
`omega dev audio` temporarily runs the plugin in your terminal in place of its
supervised process. `omega logs audio` shows its captured output.

The [audio](crates/omega/examples/audio.rs), [Wi-Fi](crates/omega/examples/wifi.rs),
and [focus timer](crates/omega/examples/focus.rs) examples show fuller plugins,
including panels, forms, and plugin-owned state.

## Documentation

See the [architecture](docs/architecture.md) for how Omega fits together,
[open design questions](docs/design.md) for work ahead, and
[contributing guidelines](AGENTS.md) for working on Omega itself. `omega --help`
lists the available commands.

## License

[MIT](LICENSE)
