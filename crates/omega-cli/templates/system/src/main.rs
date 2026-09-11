//! Rust is the source of truth for the shell layout and plugin instances.
use omega_document::shell::{Bar, Native, Shell};
use omega_document::{Document, Host};

fn main() -> anyhow::Result<()> {
    System::document()?.emit()?;
    Ok(())
}

struct System;

impl System {
    fn document() -> anyhow::Result<Document> {
        Ok(Document::new()
            .env("OMEGA_HOST", Host::name())
            .shell(
                Shell::new().bar(
                    Bar::top()
                        .left([Native::menu().into(), Native::workspaces().into()])
                        .center([Native::clock().into()])
                        .right([Native::tray().into(), Native::power().into()]),
                ),
            )?)
    }
}
