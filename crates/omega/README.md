# Omega for Rust

Build widgets, controls, and automations for the Omarchy desktop in Rust.
Omega runs your plugins, supplies system state, and renders their declarative
interfaces through Omarchy's shell.

This is the plugin SDK. The package is named `omega-rs`; Rust code imports it as
`omega`. `omega new <name>` creates a plugin with the dependency configured.
The [Omega CLI](https://crates.io/crates/omega-cli) builds and runs your config.

## A widget

A plugin declares what it reads by holding the corresponding domain handles.
Omega subscribes to those topics and redraws the widget when their state changes.

```rust
use omega::power::Battery;
use omega::ui::Text;
use omega::{Ui, Widget};

#[derive(omega::Widget)]
struct Charge {
    battery: Battery,
}

impl Widget for Charge {
    fn render(&self) -> Ui {
        if !self.battery.has_reading() {
            return Ui::empty();
        }
        Text::new(self.battery.charge()).into()
    }
}

fn main() -> omega::Result<()> {
    omega::plugin!().widget::<Charge>().run()
}
```

Domain modules put related state and controls together: `audio::{Audio, Volume}`,
`network::{Wifi, WifiControl}`, and `power::{PowerProfiles, SetProfile}`.
Controls bind to typed commands, such as `Slider::new(level).on_change(SetVolume)`.
Commands and reactions may perform actions; widgets may only read state. Derives
check that distinction and declare the capabilities the plugin needs.

`ui` provides layouts, controls, forms, and semantic styling. `config` handles
construction settings, `record` holds plugin state, and `testing` lets you render
widgets and exercise commands without a running daemon. Shared values such as
`Percent` are available at the root.

Omega targets Linux and requires Rust 1.88 or newer. Visible widgets currently
use Omarchy's Quickshell shell.

[API reference](https://docs.rs/omega-rs) ·
[Examples](https://github.com/roushou/omega/tree/main/crates/omega/examples) ·
[Project and setup](https://github.com/roushou/omega)

Licensed under MIT.
