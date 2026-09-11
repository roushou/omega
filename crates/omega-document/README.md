# Omega document

Describe the desktop configuration Omega should apply: plugin settings, widget
placements, keybindings, and schedules. This crate is the authoring API for the
`system/` crate in an Omega configuration workspace.

The document describes desired state. Constructing it does not change the desktop;
`omega build` evaluates it, and the daemon applies the result.

```rust
use omega_document::{Bars, Document, Modules};

fn main() -> Result<(), omega_document::DocumentError> {
    Document::new()
        .bar(Bars::top(
            "main",
            vec![Modules::plain_widget("clock", "clock")],
        ))
        .emit()
}
```

This declares a placement for a built plugin named `clock`. A configuration can
also depend on a plugin's library and pass its typed settings to
`Modules::widget`. `Units` configures plugin lifecycle and settings; `Schedules`
and `Actions` describe recurring work. The daemon validates references against
the plugins in the build.

Use [omega-rs](https://crates.io/crates/omega-rs) to implement plugins and
[omega-cli](https://crates.io/crates/omega-cli) to create and build the workspace.
This crate also provides document serialization and validation for Omega's host
components.

[API reference](https://docs.rs/omega-document) ·
[Project](https://github.com/roushou/omega)

Licensed under MIT.
