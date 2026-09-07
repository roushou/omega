//! The configuration plane: what this machine should be.
//!
//! One entry point, no side effects. It computes a state document and emits
//! it; `omega build` stages it, and the daemon converges the machine toward
//! it. Nothing here runs on the machine — declaring is the whole job.
//!
//! It depends on the plugins it configures, because they are crates in this
//! same workspace. So a plugin's settings are its own type, checked here: a
//! misspelled setting is a build error rather than a default quietly taken
//! on the machine at three in the morning.
//!
//! The bar below is empty because nothing has been written yet. `omega new
//! <name>` scaffolds a plugin and prints the line that puts it here.

use omega_document::{Bars, Document, Host};

fn main() -> anyhow::Result<()> {
    System::document().emit()?;
    Ok(())
}

struct System;

impl System {
    fn document() -> Document {
        Document::new()
            .env("OMEGA_HOST", Host::name())
            .bar(Bars::top("main", vec![]))
    }
}
