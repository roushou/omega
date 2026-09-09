// The unit `omega new` writes, compiled.
//
// The template is `include_str!` into the CLI and handed to a person as text,
// so nothing ever built it: after the manifest became a schema message its
// `capabilities` assertion compared a `Vec<i32>` against strings, and a
// scaffolded plugin's own tests would not compile. Nobody noticed, because
// noticing meant running `omega new` and then `cargo test` in the result.
//
// Including it here makes the scaffold a compiled artifact of this workspace:
// its widget builds against the SDK it is written for and its tests run in
// this suite, so the first thing a new author is handed is something that
// works.
//
// `#[path]` rather than `include!`: the template opens with its own `//!`,
// which is a module's documentation and only legal if the file *is* the
// module.

// The template's items are public because they are a crate's root in the
// config workspace it gets written into; here it is a private module, and
// nothing in this file calls `UNIT`. That is the template being a template,
// not dead code.
#[allow(dead_code, unreachable_pub)]
#[path = "../../omega-cli/templates/unit/src/lib.rs"]
mod scaffolded;
