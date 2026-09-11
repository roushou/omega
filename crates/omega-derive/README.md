# Omega derives

Procedural macros for Omega plugins. The [plugin SDK](https://crates.io/crates/omega-rs)
re-exports these derives, so plugin authors use `#[derive(omega::Widget)]`,
`#[derive(omega::Command)]`, and the other derives without adding this crate
directly.

`Widget`, `Command`, and `Reaction` derive construction and dependency declarations
from a surface's fields. `Command` also supplies its typed command identity.
`Config` reads construction settings, `UnitState` defines plugin record identity
and serialization, `Input` decodes structured command arguments, and `Form`
defines typed text forms.

Generated code uses the SDK's `omega::internal` contracts. Keep this crate aligned
with the SDK release that depends on it; it is not a standalone macro framework.

[SDK reference](https://docs.rs/omega-rs) ·
[Project](https://github.com/roushou/omega)

Licensed under MIT.
