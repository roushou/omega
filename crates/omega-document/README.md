# Omega document

Describe the desktop configuration Omega should apply: plugin settings, widget
placements, keybindings, and schedules. This crate is the authoring API for the
`system/` crate in an Omega configuration workspace.

The document describes desired state. Constructing it does not change the desktop;
`omega build` evaluates it, and the daemon applies the result.

```rust
use omega_document::Document;
use omega_document::shell::{Bar, Native, PluginWidget, Shell};

fn main() -> omega_document::Result<()> {
    Document::new()
        .shell(Shell::new().bar(
            Bar::top()
                .left([Native::menu().into(), Native::workspaces().into()])
                .center([Native::clock().into()])
                .right([
                    PluginWidget::new("audio", "audio")
                        .surface("indicator")
                        .panel("panel")
                        .into(),
                ]),
        ))?
        .emit()?;
    Ok(())
}
```

A plugin placement generates both its Omarchy bar entry and its Omega render
instances. Its ID distinguishes placements of the same plugin; `settings(&value)`
takes the plugin's typed settings. Native widgets share the same ordered layout.
`Idle` configures inactivity deadlines, and `Native::options` accepts serializable
options for native plugins. Explicit extensions preserve settings without typed
APIs and cannot replace fields owned by typed configuration.

Shell ownership is opt-in. `omega shell adopt` backs up the current file and
generates an editable Rust module without changing the desktop. Once adopted,
`omega build` stages the generated shell configuration with the plugins; the
daemon applies it. External edits are preserved and reported as conflicts.
Inspect them with `omega shell diff`; `omega shell apply --overwrite` explicitly
restores the Rust-defined configuration. Invalid JSON must be repaired before
application, even with overwrite.

Legacy `Document::bar` declares render instances only. It cannot be combined with
`Document::shell`. `Units` configures lifecycle and settings; `Schedules` and
`Actions` describe recurring work. The daemon validates references against the
plugins in the build.

Configuration authors can use `omega_document::Result` directly; no error-reporting
crate is required. The concrete errors also integrate with application-selected
error libraries through `std::error::Error`.

Use [omega-rs](https://crates.io/crates/omega-rs) to implement plugins and
[omega-cli](https://crates.io/crates/omega-cli) to create and build the workspace.
This crate also provides document serialization and validation for Omega's host
components.

[API reference](https://docs.rs/omega-document) ·
[Project](https://github.com/roushou/omega)

Licensed under MIT.
