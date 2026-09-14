# Omega for Rust

Build desktop interfaces, controls, and automations in Rust. Omega runs your
plugins, supplies system state, and renders their declarative interfaces in
Omarchy’s shell or independent Quickshell windows and overlays.

This is the plugin SDK. The package is named `omega-rs`; Rust code imports it as
`omega`. `omega new <name>` creates a plugin with the dependency configured.
The [Omega CLI](https://crates.io/crates/omega-cli) builds and runs your config.

## A surface

A plugin declares what it reads by holding the corresponding domain handles.
Omega subscribes to those topics and redraws the widget when their state changes.

```rust
use omega::platform::power::Battery;
use omega::ui::Text;
use omega::{Surface, View};

#[derive(omega::Surface)]
struct Charge {
    battery: Battery,
}

impl Surface for Charge {
    fn render(&self) -> View {
        if !self.battery.has_reading() {
            return View::empty();
        }
        Text::new(self.battery.charge()).into()
    }
}

fn main() -> omega::Result<()> {
    omega::plugin!().surface(Charge).run()
}
```

Service domains own related state and controls under `omega::platform`:
`audio::{Audio, Volume}`, `network::{Wifi, WifiControl}`, and
`power::{PowerProfiles, SetProfile}`.
Controls bind to typed commands, such as `Slider::new(level).on_change(SetVolume)`.
Commands and reactions may perform actions; render declarations only read state.
`StatefulSurface` adds an instance-local model, typed messages, and managed tasks
with separate effect dependencies. Derives enforce that separation and declare
the capabilities the plugin needs.

`ui` provides layouts, controls, forms, and semantic styling. `config` handles
construction settings, `record` holds plugin state, and `testing` lets you render
widgets and exercise commands without a running daemon. Shared values such as
`Percent` are available at the root. Typed references live in
`surface::SurfaceRef` and `command::CommandRef`. Omega’s supervision readings
live in `plugin::{Units, UnitPhase, UnitReport}`; `platform::system` describes
machine resources.

See [`examples/media.rs`](examples/media.rs) for a media indicator and panel
with typed icons, player selection, and playback commands. `Media` reads;
`MediaControl` acts through the daemon. Bind `PlayerId` directly to a command
so a click targets the displayed endpoint even if the active player changes.

Omega targets Linux and requires Rust 1.88 or newer. Quickshell renders surfaces;
Omarchy supplies the integrated bar host. The `omega-preview` dev dependency
provides isolated component and surface cases without a daemon.

[Authoring guide](https://github.com/roushou/omega/blob/main/docs/authoring.md) ·
[API reference](https://docs.rs/omega-rs) ·
[Examples](https://github.com/roushou/omega/tree/main/crates/omega/examples) ·
[Project and setup](https://github.com/roushou/omega)

Licensed under MIT.
