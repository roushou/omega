# Omega document

Host-independent authoring, serialization, and validation of Omega desired state.
The configuration's `system/` crate computes this document; the daemon applies it.

```rust
use omega_document::{Document, Units};

fn main() -> omega_document::Result<()> {
    Document::new()
        .unit(Units::enabled("audio"))
        .env("DESKTOP_NAME", "My desktop")
        .emit()
}
```

`Document::with` composes typed integrations implementing `DocumentExtension`.
Omarchy shell layouts live in [omega-omarchy](../omega-omarchy), with
`Document::new().with(Shell::new())?`. Core validation rejects an unhandled shell
payload; the Omarchy adapter validates it and its projected widget instances
before delegating to core validation.

Licensed under MIT.
