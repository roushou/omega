//! The program. Everything this plugin is lives in the library beside it, so
//! `system/` can depend on it and be checked against it.

fn main() -> omega::Result<()> {
    {unit_snake}::plugin().run()
}
