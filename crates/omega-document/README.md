# Omega document

Host-independent authoring, serialization, and validation of Omega desired state.
The configuration's `system/` crate computes this document; the daemon applies it.

```rust
use omega_document::{Document, Plugins};

fn main() -> omega_document::Result<()> {
    Document::new()
        .plugin(Plugins::enabled("audio"))
        .env("DESKTOP_NAME", "My desktop")
        .emit()
}
```

`Document::with` composes typed integrations implementing `DocumentExtension`.
Omarchy shell layouts live in [omega-omarchy](../omega-omarchy), with
`Document::new().with(Shell::new())?`. Core validation rejects an unhandled shell
payload; the Omarchy adapter validates it and its projected widget instances
before delegating to core validation.

Schedules and keybindings share typed command actions. With the command library
as a Rust dependency, `Actions::invoke(focus::Tick)` uses `Tick::ID`;
`Actions::invoke_with(audio::SetVolume, Percent::whole(50))` also checks the input
type. The daemon resolves the configured provider and checks its registration.
Dynamic targets use the explicit `invoke_named` / `invoke_named_with` methods.

`Document::command_host` accepts a `CommandHost` declaration with default
deployment settings, or a `CommandHostDeployment` from `.deployment()` for
explicit lifetime, execution limits, and settings. These declarations configure
the daemon; importing a command library starts no process.

Licensed under MIT.
