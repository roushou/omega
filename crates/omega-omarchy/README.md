# Omega Omarchy integration

Configure Omarchy's shell layout, native widgets, Omega widget placements, and
idle behavior from your config's `system/` crate. Compose `Shell` into an
`omega_document::Document`; `omega build` evaluates it and the daemon applies it.

Plugin settings and schedules belong to `omega-document`. This crate provides
Omarchy-specific authoring and host integration.

```rust
use omega_document::Document;
use omega_omarchy::shell::{Bar, Native, PluginWidget, Shell};

fn main() -> omega_document::Result<()> {
    Document::new()
        .with(Shell::new().bar(
            Bar::top()
                .left([Native::menu().into(), Native::workspaces().into()])
                .center([Native::clock().into()])
                .right([
                    PluginWidget::new("audio", audio::Indicator)
                        .panel(audio::Panel)
                        .into(),
                ]),
        ))?
        .emit()?;
    Ok(())
}
```

`audio::Indicator` and `audio::Panel` are public widget types from the audio
plugin. `#[derive(Surface)]` supplies their references; the plugin registers them
with `.surface(Indicator).surface(Panel)`. Imported or dynamic configuration can use
`PluginWidget::named(id, plugin).surface_named(surface).panel_named(panel)`.

A plugin placement generates both its Omarchy bar entry and its Omega render
instances. Its ID distinguishes placements of the same plugin; `settings(&value)`
takes the plugin's typed settings. Native widgets share the same ordered layout.
`Idle` configures inactivity deadlines, and `Native::options` accepts serializable
options for native plugins. Explicit extensions preserve settings without typed
APIs and cannot replace fields owned by typed configuration.

## Generated bar entries

Omarchy selects the widget through `id`. Omega's adapter reads its settings from
one nested `omega` object:

```json
{
  "id": "omega.view",
  "omega": {
    "placement": "audio-main",
    "plugin": "audio",
    "surface": "indicator",
    "panel": "panel"
  }
}
```

`placement` identifies the configured slot; `plugin` identifies the process that
provides its UI. `surface` selects the bar content, and optional `panel` selects
its popup surface. Two placements can use the same plugin. Omega settings stay
inside this namespace so they cannot collide with Omarchy's top-level widget
settings. The adapter also accepts `omega.socket` to override the observation
socket for a manually configured entry.

Generate this file through `Shell` rather than editing it alongside the Rust
configuration. Import expects the nested format and refuses unsupported Omega
options instead of dropping them.

## Shell ownership

Shell ownership is opt-in. `omega shell adopt` backs up the current file and
generates an editable Rust module without changing the desktop. Once adopted,
`omega build` stages the generated shell configuration with the plugins; the
daemon applies it. External edits are preserved and reported as conflicts.
Inspect them with `omega shell diff`; `omega shell apply --overwrite` explicitly
restores the Rust-defined configuration. Invalid JSON must be repaired before
application, even with overwrite.

Legacy `Document::bar` declares render instances only. It cannot be combined with
`Document::with(Shell)`. `Plugins` configures lifecycle and settings; `Schedules` and
`Actions` describe recurring work. The daemon validates references against the
plugins in the build.

Configuration authors can use `omega_document::Result` directly; no error-reporting
crate is required. The concrete errors also integrate with application-selected
error libraries through `std::error::Error`.

Use [omega-rs](https://crates.io/crates/omega-rs) to implement plugins and
[omega-cli](https://crates.io/crates/omega-cli) to create and build the workspace.
This crate also compiles and validates Omarchy declarations and installs the
host adapter.

[API reference](https://docs.rs/omega-omarchy) ·
[Project](https://github.com/roushou/omega)

Licensed under MIT.
